use anyhow::{Result, bail};
use chrono::{Duration, Utc};
use colored::Colorize;

use crate::config::Config;
use crate::fetch::fetch_feed;
use crate::state::{Feed, Item, State};
use crate::{Cli, Cmd};

pub fn run_command(cli: Cli, cfg: &Config, state: &mut State) -> Result<()> {
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
            cmd_show_feed(state, cfg, &id_or_url, limit)?;
        }
        Some(Cmd::Refresh { ids_or_urls }) => {
            cmd_refresh(state, cfg, &ids_or_urls)?;
        }
        Some(Cmd::Rename { key, alias }) => {
            cmd_rename(state, &key, &alias)?;
        }
        None => {
            // default: show recent items across all feeds
            cmd_show_all(state, cfg, limit)?;
        }
    }

    Ok(())
}

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

/// Refresh one feed if its cache is stale
fn refresh_feed_if_needed(state: &mut State, feed_index: usize, cfg: &Config) -> Result<()> {
    let now = Utc::now();
    let refresh_after = Duration::minutes(cfg.refresh_age_mins as i64);

    // Get a mutable reference to this feed
    let feed = &mut state.feeds[feed_index];

    let needs_refresh = match feed.last_fetched_at {
        None => true,
        Some(last) => now - last >= refresh_after,
    };

    if !needs_refresh {
        return Ok(());
    }

    match fetch_feed(feed) {
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

/// Default `rsso` behaviour: show recent items across all feeds
fn cmd_show_all(state: &mut State, cfg: &Config, limit: usize) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed. Use `rsso sub <url>` to add one.");
        return Ok(());
    }

    // Refresh all feeds if needed
    for idx in 0..state.feeds.len() {
        refresh_feed_if_needed(state, idx, cfg)?;
    }

    // Clone items so we can sort without touching original order
    let mut items = state.items.clone();

    // Sort newest first, using published_at or updated_at or first_seen_at
    items.sort_by(|a, b| {
        let a_date = a
            .published_at
            .unwrap_or(a.updated_at.unwrap_or(a.first_seen_at));
        let b_date = b
            .published_at
            .unwrap_or(b.updated_at.unwrap_or(b.first_seen_at));
        b_date.cmp(&a_date)
    });

    // Limit and print
    for item in items.into_iter().take(limit) {
        print_item_line(&item, state, cfg);
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
fn cmd_show_feed(state: &mut State, cfg: &Config, key: &str, limit: usize) -> Result<()> {
    // Find index of the matching feed using alias OR title OR id OR url
    let feed_index = match state.find_feed_index(key) {
        Some(i) => i,
        None => {
            bail!("No matching feed for '{}'", key);
        }
    };

    // Refresh that single feed if needed
    refresh_feed_if_needed(state, feed_index, cfg)?;

    let feed_id = state.feeds[feed_index].id.clone();

    // Collect & sort items only for this feed
    let mut items: Vec<Item> = state
        .items
        .iter()
        .filter(|i| i.feed_id == feed_id)
        .cloned()
        .collect();

    items.sort_by(|a, b| {
        let a_date = a
            .published_at
            .unwrap_or(a.updated_at.unwrap_or(a.first_seen_at));
        let b_date = b
            .published_at
            .unwrap_or(b.updated_at.unwrap_or(b.first_seen_at));
        b_date.cmp(&a_date)
    });

    for item in items.into_iter().take(limit) {
        print_item_line(&item, state, cfg);
    }

    Ok(())
}

/// Refresh all feeds, or a selected subset
fn cmd_refresh(state: &mut State, cfg: &Config, keys: &[String]) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed.");
        return Ok(());
    }

    if keys.is_empty() {
        // Refresh all
        for idx in 0..state.feeds.len() {
            refresh_feed_if_needed(state, idx, cfg)?;
        }
        println!("Refreshed all feeds.");
    } else {
        // Refresh only selected feeds
        for key in keys {
            match state.find_feed_index(key) {
                Some(i) => {
                    refresh_feed_if_needed(state, i, cfg)?;
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
