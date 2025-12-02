use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::create_dir_all;
use std::path::Path;

use crate::config::Config;

/// A subscribed feed
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Feed {
    pub id: String, // alias or generated id
    pub url: String,
    pub alias: Option<String>,
    pub title: Option<String>,
    pub added_at: DateTime<Utc>,
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// A single item/article in a feed
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Item {
    pub feed_id: String,
    pub title: String,
    pub link: String,
    pub published_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub first_seen_at: DateTime<Utc>,
}

/// Entire app state that gets serialized to JSON
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub feeds: Vec<Feed>,
    pub items: Vec<Item>,
}

/// Load state from the JSON file, or return an empty state if it doesn't exist.
pub fn load_state(cfg: &Config) -> Result<State> {
    let path = &cfg.state_path;
    if !Path::new(path).exists() {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        return Ok(State::default());
    }

    let contents = fs::read_to_string(path)?;
    if contents.trim().is_empty() {
        return Ok(State::default());
    }

    let state: State = serde_json::from_str(&contents)?;
    Ok(state)
}

/// Save state to the JSON file.
pub fn save_state(cfg: &Config, state: &State) -> Result<()> {
    let path = &cfg.state_path;
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    fs::write(path, json)?;
    Ok(())
}

impl State {
    /// Find a mutable reference to a feed by id, alias, or url.
    pub fn find_feed_mut(&mut self, id_or_url: &str) -> Option<&mut Feed> {
        self.feeds.iter_mut().find(|f| {
            f.id == id_or_url || f.alias.as_deref() == Some(id_or_url) || f.url == id_or_url
        })
    }

    /// Find an immutable reference to a feed by id, alias, or url.
    pub fn find_feed(&self, id_or_url: &str) -> Option<&Feed> {
        self.feeds.iter().find(|f| {
            f.id == id_or_url || f.alias.as_deref() == Some(id_or_url) || f.url == id_or_url
        })
    }

    /// Add a feed; error if same id or url already exists.
    pub fn add_feed(&mut self, feed: Feed) -> Result<()> {
        if self
            .feeds
            .iter()
            .any(|f| f.url == feed.url || f.id == feed.id)
        {
            anyhow::bail!("Feed already exists");
        }
        self.feeds.push(feed);
        Ok(())
    }

    /// Remove a feed by id/alias/url and drop its items.
    /// Returns how many feeds were removed (0 or 1).
    pub fn remove_feed(&mut self, id_or_url: &str) -> usize {
        // Collect IDs of feeds we’re about to remove
        let mut removed_ids: Vec<String> = Vec::new();

        self.feeds.retain(|f| {
            let to_remove =
                f.id == id_or_url || f.alias.as_deref() == Some(id_or_url) || f.url == id_or_url;

            if to_remove {
                removed_ids.push(f.id.clone());
            }

            !to_remove
        });

        if !removed_ids.is_empty() {
            self.items
                .retain(|i| !removed_ids.iter().any(|id| &i.feed_id == id));
        }

        removed_ids.len()
    }
}
