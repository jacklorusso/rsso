use anyhow::{Result, bail};
use chrono::{Duration, Utc};
use colored::Colorize;
use futures::{StreamExt, stream};
use reqwest::Client;

use crate::config::Config;
use crate::fetch::fetch_feed;
use crate::state::{Feed, Item, State};
use crate::{Cli, Cmd};

pub async fn run_command(cli: Cli, cfg: &Config, state: &mut State) -> Result<()> {
    let limit = cli.limit.unwrap_or(cfg.default_limit);

    match cli.command {
        Some(Cmd::Sub { url, alias }) => {
            cmd_sub(state, &url, alias)?;
        }
        Some(Cmd::Unsub { id_or_url }) => {
            cmd_unsub(state, &id_or_url)?;
        }
        Some(Cmd::List) => {
            cmd_list(state)?;
        }
        Some(Cmd::Feed { id_or_url }) => {
            cmd_show_feed(state, cfg, &id_or_url, limit).await?;
        }
        Some(Cmd::Refresh { ids_or_urls }) => {
            cmd_refresh(state, cfg, &ids_or_urls).await?;
        }
        Some(Cmd::Rename { key, alias }) => {
            cmd_rename(state, &key, &alias)?;
        }
        None => {
            // default: show recent items across all feeds
            cmd_show_all(state, cfg, limit).await?;
        }
    }

    Ok(())
}

fn build_http_client() -> Result<Client> {
    let client = Client::builder()
        .user_agent("rsso")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    Ok(client)
}

/// Refresh multiple feeds concurrently, with a bounded concurrency limit.
///
/// This function solves two problems:
/// 1. We want to fetch many feeds in parallel.
/// 2. We cannot hold &mut State or &mut Feed across .await points.
///
/// The solution:
/// - First: decide *which* feeds need refreshing, and clone those Feed values.
/// - Second: run all network fetches concurrently using the cloned feeds.
/// - Third: after all await points, re-borrow `state` mutably and apply results.
async fn refresh_feeds_concurrent<I>(
    state: &mut State,
    cfg: &Config,
    client: &Client,
    indices: I, // iterable of feed indices, e.g. 0..state.feeds.len()
) -> Result<()>
where
    I: IntoIterator<Item = usize>,
{
    let now = Utc::now();
    let refresh_after = Duration::minutes(cfg.refresh_age_mins as i64);

    // ---------------------------------------------------------
    // STEP 1: Determine which feeds are stale and clone them.
    // ---------------------------------------------------------
    //
    // We cannot pass &mut Feed into async tasks because that would
    // require holding a mutable reference across .await, which Rust forbids.
    //
    // So we clone each stale Feed into a list; these clones will be used
    // purely for network fetching.
    //
    let mut to_refresh: Vec<(usize, Feed)> = Vec::new();

    for idx in indices {
        let feed = &state.feeds[idx];

        // Staleness rule: never fetched OR last fetch older than refresh_after
        let needs_refresh = match feed.last_fetched_at {
            None => true,
            Some(last) => now - last >= refresh_after,
        };

        if needs_refresh {
            // Clone the feed so we can send it into async tasks
            to_refresh.push((idx, feed.clone()));
        }
    }

    // Nothing to do — all feeds are fresh
    if to_refresh.is_empty() {
        return Ok(());
    }

    // ---------------------------------------------------------
    // STEP 2: Concurrently fetch all stale feeds.
    // ---------------------------------------------------------
    //
    // buffer_unordered(concurrency) ensures:
    // - Up to a set limit of fetches happen at once
    // - Results are returned as they finish (not in original order)
    //
    // Each task gets:
    // - The cloned feed (safe across .await)
    // - A cloned reqwest Client (cheap; internal pool is shared)
    //
    let concurrency_limit: usize = 20;

    let results: Vec<(usize, Result<(Option<String>, Vec<Item>)>)> = stream::iter(to_refresh)
        .map(|(idx, feed_clone)| {
            // Clone client for use inside the async block
            let client = client.clone();

            async move {
                // Asynchronously fetch using the cloned feed
                let res = fetch_feed(&client, &feed_clone).await;
                (idx, res)
            }
        })
        .buffer_unordered(concurrency_limit)
        .collect()
        .await;

    // ---------------------------------------------------------
    // STEP 3: Apply results back to the real, mutable State.
    // ---------------------------------------------------------
    //
    // After all .await points have finished, we now re-borrow
    // the real feeds/items inside State and update them safely.
    //
    // No borrow checker issues here because we only hold &mut references
    // *after* all async operations are complete.
    //
    for (idx, fetch_result) in results {
        let feed = &mut state.feeds[idx];

        match fetch_result {
            Ok((title_opt, mut new_items)) => {
                // Update title if provided
                if let Some(t) = title_opt {
                    feed.title = Some(t);
                }

                // Mark feed as successfully fetched
                feed.last_fetched_at = Some(now);
                feed.last_error = None;

                // Replace old items for this feed
                let feed_id = feed.id.clone();
                state.items.retain(|i| i.feed_id != feed_id);

                // Add the freshly fetched items
                state.items.append(&mut new_items);
            }

            Err(err) => {
                // Mark this feed as failed
                feed.last_error = Some(err.to_string());
            }
        }
    }

    Ok(())
}

/// Refresh one feed if its cache is stale
async fn refresh_feed_if_needed(
    state: &mut State,
    feed_index: usize,
    cfg: &Config,
    client: &Client,
) -> Result<()> {
    let now = Utc::now();
    let refresh_after = Duration::minutes(cfg.refresh_age_mins as i64);

    // Take a snapshot of the feed to decide if we need to refresh
    // and to pass to fetch_feed without holding a &mut borrow across .await
    let (needs_refresh, feed_snapshot) = {
        let feed = &state.feeds[feed_index];
        let needs_refresh = match feed.last_fetched_at {
            None => true,
            Some(last) => now - last >= refresh_after,
        };
        (needs_refresh, feed.clone())
    };

    if !needs_refresh {
        return Ok(());
    }

    // Perform the network request asynchronously using the snapshot
    let fetch_result = fetch_feed(client, &feed_snapshot).await;

    // Re-borrow the original feed mutably to apply changes
    let feed = &mut state.feeds[feed_index];

    match fetch_result {
        Ok((title_opt, mut new_items)) => {
            if let Some(t) = title_opt {
                feed.title = Some(t);
            }
            feed.last_fetched_at = Some(now);
            feed.last_error = None;

            // Drop old items for this feed
            let feed_id = feed.id.clone();
            state.items.retain(|i| i.feed_id != feed_id);

            // Add the new items
            state.items.append(&mut new_items);
        }
        Err(err) => {
            feed.last_error = Some(err.to_string());
        }
    }

    Ok(())
}

/// Print a single item in pipe-friendly format
fn print_item_line(item: &Item, state: &State, cfg: &Config) {
    let date = item
        .published_at
        .unwrap_or(item.updated_at.unwrap_or(item.first_seen_at))
        .format("%d %b %y")
        .to_string();

    let feed_label = state
        .feeds
        .iter()
        .find(|f| f.id == item.feed_id)
        .and_then(|f| {
            f.alias
                .clone()
                .or_else(|| f.title.clone())
                .or_else(|| Some(f.id.clone()))
        })
        .unwrap_or_else(|| item.feed_id.clone());

    println!(
        "{} | {} | {} | {}",
        date,
        feed_label,
        item.title.bold(),
        item.link.blue()
    );
    if cfg.new_line_between_items {
        println!("");
    }
}

fn sort_items_newest_first(items: &mut Vec<&Item>) {
    items.sort_by(|a, b| {
        let a_date = a
            .published_at
            .unwrap_or(a.updated_at.unwrap_or(a.first_seen_at));
        let b_date = b
            .published_at
            .unwrap_or(b.updated_at.unwrap_or(b.first_seen_at));
        b_date.cmp(&a_date)
    });
}

// COMMANDS

/// Subscribe to a new feed
fn cmd_sub(state: &mut State, url: &str, alias: Option<String>) -> Result<()> {
    // crude id: use alias if provided, otherwise derive from URL
    let id = alias.clone().unwrap_or_else(|| {
        url.replace("https://", "")
            .replace("http://", "")
            .trim_end_matches('/')
            .replace('/', "-")
    });

    let feed = Feed {
        id: id.clone(),
        url: url.to_string(),
        alias,
        title: None, // will be filled on first fetch
        added_at: Utc::now(),
        last_fetched_at: None,
        last_error: None,
    };

    state.add_feed(feed)?;
    println!("Subscribed to {}", url);
    Ok(())
}

/// Unsubscribe from a feed using alias/title/id/url
fn cmd_unsub(state: &mut State, key: &str) -> Result<()> {
    let removed = state.remove_feed(key);
    if removed == 0 {
        bail!("No matching feed for '{}'", key);
    } else {
        println!("Unsubscribed {}", key);
        Ok(())
    }
}

/// List subscribed feeds with status
fn cmd_list(state: &State) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed. Use `rsso sub <url>` to add one.");
        return Ok(());
    }

    for f in &state.feeds {
        let id = &f.id;
        let name = f.title.as_deref().unwrap_or(&f.url);
        let status = if let Some(err) = &f.last_error {
            format!("ERROR: {}", err)
        } else if let Some(last) = f.last_fetched_at {
            format!("OK (last fetched: {})", last.to_rfc3339())
        } else {
            "Never fetched".to_string()
        };

        println!("{id} | {name} | {} | {status}", f.url);
    }
    Ok(())
}

/// Rename a feed's alias (and internal id), matched by alias/title/id/url
fn cmd_rename(state: &mut State, key: &str, new_alias: &str) -> Result<()> {
    let new_alias = new_alias.trim();
    if new_alias.is_empty() {
        bail!("Alias cannot be empty");
    }

    // Make sure no other feed already uses this alias (case-insensitive)
    let new_lower = new_alias.to_lowercase();
    if state
        .feeds
        .iter()
        .any(|f| f.alias.as_ref().map(|a| a.to_lowercase()).as_deref() == Some(new_lower.as_str()))
    {
        bail!("Alias '{}' is already in use", new_alias);
    }

    // Find the feed by alias/title/id/url
    let idx = match state.find_feed_index(key) {
        Some(i) => i,
        None => bail!("No matching feed for '{}'", key),
    };

    let old_id = state.feeds[idx].id.clone();

    // Update alias and id to the new alias
    state.feeds[idx].alias = Some(new_alias.to_string());
    state.feeds[idx].id = new_alias.to_string();

    // Update items to reference the new feed id
    for item in state.items.iter_mut() {
        if item.feed_id == old_id {
            item.feed_id = new_alias.to_string();
        }
    }

    println!("Renamed feed '{}' to alias '{}'", key, new_alias);
    Ok(())
}

/// Default `rsso` behaviour: show recent items across all feeds
async fn cmd_show_all(state: &mut State, cfg: &Config, limit: usize) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed. Use `rsso sub <url>` to add one.");
        return Ok(());
    }

    // Build a shared HTTP client
    let client = build_http_client()?;

    // Refresh all feeds concurrently (only those that are stale)
    let indices: Vec<usize> = (0..state.feeds.len()).collect();
    refresh_feeds_concurrent(state, cfg, &client, indices).await?;

    // Build a vector of references (we used to clone items but this is faster)
    let mut items: Vec<&Item> = state.items.iter().collect();

    sort_items_newest_first(&mut items);

    for item in items.into_iter().take(limit) {
        print_item_line(item, state, cfg);
    }

    // After printing items, show a warning if any feeds had errors
    let failing: Vec<_> = state
        .feeds
        .iter()
        .filter(|f| f.last_error.is_some())
        .collect();

    if !failing.is_empty() {
        eprintln!();
        eprintln!("Warning: {} feed(s) had errors:", failing.len());
        for f in failing {
            let label = f
                .alias
                .clone()
                .or_else(|| f.title.clone())
                .unwrap_or_else(|| f.id.clone());
            eprintln!(
                "- {} ({})",
                label,
                f.last_error.as_deref().unwrap_or("unknown error")
            );
        }
        eprintln!("Run `rsso list` for more details.");
    }

    Ok(())
}

/// Show recent items for a single feed
async fn cmd_show_feed(state: &mut State, cfg: &Config, key: &str, limit: usize) -> Result<()> {
    // Find index of the matching feed using alias OR title OR id OR url
    let feed_index = match state.find_feed_index(key) {
        Some(i) => i,
        None => {
            bail!("No matching feed for '{}'", key);
        }
    };

    let client = build_http_client()?;

    // Refresh that single feed if needed
    refresh_feed_if_needed(state, feed_index, cfg, &client).await?;

    let feed_id = state.feeds[feed_index].id.clone();

    // Collect & sort items only for this feed
    let mut items: Vec<&Item> = state
        .items
        .iter()
        .filter(|i| i.feed_id == feed_id)
        .collect();

    sort_items_newest_first(&mut items);

    for item in items.into_iter().take(limit) {
        print_item_line(item, state, cfg);
    }

    Ok(())
}

/// Refresh all feeds, or a selected subset
async fn cmd_refresh(state: &mut State, cfg: &Config, keys: &[String]) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed.");
        return Ok(());
    }

    let client = build_http_client()?;

    if keys.is_empty() {
        // No specific keys: refresh all feeds concurrently
        let indices: Vec<usize> = (0..state.feeds.len()).collect();
        refresh_feeds_concurrent(state, cfg, &client, indices).await?;
        println!("Refreshed all feeds.");
    } else {
        // Keys were provided: refresh only selected feeds (sequentially is fine)
        for key in keys {
            match state.find_feed_index(key) {
                Some(i) => {
                    refresh_feed_if_needed(state, i, cfg, &client).await?;
                    println!("Refreshed {}", key);
                }
                None => {
                    eprintln!("No matching feed for '{}'", key);
                }
            }
        }
    }

    Ok(())
}
