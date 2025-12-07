use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use colored::Colorize;
use opml::OPML;
use std::fs;
use std::path::{Path, PathBuf};

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
        Some(Cmd::Import { path }) => {
            cmd_import_opml(state, &path)?;
        }
        Some(Cmd::Export { output }) => {
            cmd_export_opml(state, cfg, output.as_deref())?;
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
    let feed = build_feed(url, alias);

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

fn build_feed(url: &str, alias: Option<String>) -> Feed {
    // crude id: use alias if provided, otherwise derive from URL
    let id = alias.clone().unwrap_or_else(|| {
        url.replace("https://", "")
            .replace("http://", "")
            .trim_end_matches('/')
            .replace('/', "-")
    });

    Feed {
        id,
        url: url.to_string(),
        alias,
        title: None, // will be filled on first fetch
        added_at: Utc::now(),
        last_fetched_at: None,
        last_error: None,
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

fn sort_items_newest_first(items: &mut Vec<Item>) {
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

    sort_items_newest_first(&mut items);

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

    sort_items_newest_first(&mut items);

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

fn default_opml_path(cfg: &Config) -> PathBuf {
    cfg.state_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("subscriptions.opml")
}

fn cmd_import_opml(state: &mut State, path: &str) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read OPML file '{}'", path))?;
    let opml: OPML = contents
        .parse()
        .with_context(|| format!("Failed to parse OPML file '{}'", path))?;

    let mut outlines = Vec::new();
    collect_outlines(&opml.body.outlines, &mut outlines);

    let mut added = 0;
    let mut skipped = 0;

    for outline in outlines {
        if let Some(xml_url) = &outline.xml_url {
            let alias = outline
                .title
                .clone()
                .or_else(|| outline.text.clone())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let feed = build_feed(xml_url, alias);

            match state.add_feed(feed) {
                Ok(_) => added += 1,
                Err(_) => skipped += 1,
            }
        }
    }

    println!(
        "Imported {added} feed(s). {} skipped because they already exist.",
        skipped
    );

    Ok(())
}

fn collect_outlines<'a>(outlines: &'a [opml::Outline], acc: &mut Vec<&'a opml::Outline>) {
    for outline in outlines {
        acc.push(outline);
        collect_outlines(&outline.outlines, acc);
    }
}

fn cmd_export_opml(state: &State, cfg: &Config, output: Option<&str>) -> Result<()> {
    if state.feeds.is_empty() {
        println!("No feeds subscribed.");
        return Ok(());
    }

    let path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_opml_path(cfg));

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let outlines: Vec<opml::Outline> = state
        .feeds
        .iter()
        .map(|f| {
            let title = f
                .alias
                .clone()
                .or_else(|| f.title.clone())
                .unwrap_or_else(|| f.id.clone());

            opml::Outline {
                text: Some(title.clone()),
                title: Some(title),
                xml_url: Some(f.url.clone()),
                html_url: None,
                r#type: Some("rss".to_string()),
                version: None,
                language: None,
                description: None,
                category: None,
                created: None,
                outline_type: None,
                url: None,
                length: None,
                outlines: vec![],
            }
        })
        .collect();

    let opml = OPML {
        head: Some(opml::Head {
            title: Some("rsso subscriptions".to_string()),
            ..Default::default()
        }),
        body: opml::Body { outlines },
        ..Default::default()
    };

    let xml = opml
        .to_string()
        .context("Failed to serialize subscriptions to OPML")?;
    fs::write(&path, xml)
        .with_context(|| format!("Failed to write OPML to '{}'", path.display()))?;

    println!(
        "Exported {} feed(s) to {}",
        state.feeds.len(),
        path.display()
    );
    Ok(())
}
