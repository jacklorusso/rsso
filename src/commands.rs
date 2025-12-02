use anyhow::{Result, bail};
use chrono::{Duration, Utc};

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
            cmd_show_feed(state, &cfg, &id_or_url, limit)?;
        }
        Some(Cmd::Refresh { ids_or_urls }) => {
            cmd_refresh(state, &cfg, &ids_or_urls)?;
        }
        None => {
            // default: show recent items across all feeds
            cmd_show_all(state, &cfg, limit)?;
        }
    }

    Ok(())
}

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

fn cmd_unsub(state: &mut State, id_or_url: &str) -> Result<()> {
    let removed = state.remove_feed(id_or_url);
    if removed == 0 {
        bail!("No matching feed for '{}'", id_or_url);
    } else {
        println!("Unsubscribed {}", id_or_url);
        Ok(())
    }
}

fn cmd_list(state: &State) -> Result<()> {
    for f in &state.feeds {
        let id = &f.id;
        let name = f.title.as_deref().unwrap_or(&f.url);
        println!("{id} | {name} | {}", f.url);
    }
    Ok(())
}

fn cmd_show_all(state: &mut State, cfg: &Config, limit: usize) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed. Use `rsso sub <url>` to add one.");
        return Ok(());
    }

    // Refresh all feeds if needed
    for idx in 0..state.feeds.len() {
        refresh_feed_if_needed(state, idx, cfg)?;
    }

    // Clone items so we can sort without touching the original order
    let mut items = state.items.clone();

    // Sort newest first, using published_at or first_seen_at
    items.sort_by(|a, b| {
        let a_date = a.published_at.unwrap_or(a.first_seen_at);
        let b_date = b.published_at.unwrap_or(b.first_seen_at);
        b_date.cmp(&a_date)
    });

    // Limit and print
    for item in items.into_iter().take(limit) {
        print_item_line(&item, state);
    }

    Ok(())
}

fn cmd_show_feed(state: &mut State, cfg: &Config, id_or_url: &str, limit: usize) -> Result<()> {
    // Find index of the matching feed
    let feed_index = state.feeds.iter().position(|f| {
        f.id == id_or_url || f.alias.as_deref() == Some(id_or_url) || f.url == id_or_url
    });

    let feed_index = match feed_index {
        Some(i) => i,
        None => {
            bail!("No matching feed for '{}'", id_or_url);
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
        let a_date = a.published_at.unwrap_or(a.first_seen_at);
        let b_date = b.published_at.unwrap_or(b.first_seen_at);
        b_date.cmp(&a_date)
    });

    for item in items.into_iter().take(limit) {
        print_item_line(&item, state);
    }

    Ok(())
}

fn cmd_refresh(state: &mut State, cfg: &Config, ids_or_urls: &[String]) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed.");
        println!("Use `rsso sub <url>` to add one!");
        return Ok(());
    }

    if ids_or_urls.is_empty() {
        // Refresh all
        for idx in 0..state.feeds.len() {
            refresh_feed_if_needed(state, idx, cfg)?;
        }
        println!("Refreshed all feeds.");
    } else {
        // Refresh only selected feeds
        for id_or_url in ids_or_urls {
            let feed_index = state.feeds.iter().position(|f| {
                f.id == *id_or_url
                    || f.alias.as_deref() == Some(id_or_url.as_str())
                    || f.url == *id_or_url
            });

            match feed_index {
                Some(i) => {
                    refresh_feed_if_needed(state, i, cfg)?;
                    println!("Refreshed {}", id_or_url);
                }
                None => {
                    eprintln!("No matching feed for '{}'", id_or_url);
                }
            }
        }
    }

    Ok(())
}

fn print_item_line(item: &Item, state: &State) {
    let date = item.published_at.unwrap_or(item.first_seen_at).to_rfc3339();

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

    println!("{} | {} | {} | {}", date, feed_label, item.title, item.link);
}
