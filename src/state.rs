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
    pub id: String, // auto-generated id (or alias if set)
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
    pub updated_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub first_seen_at: DateTime<Utc>,
}

/// Entire app state that gets serialized to JSON
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub feeds: Vec<Feed>,
    pub items: Vec<Item>,
}

/// Load state from JSON (or create an empty one)
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

/// Save state to JSON
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
    /// Match by alias OR title (case-insensitive),
    /// fallback to exact id/url match.
    fn feed_matches(f: &Feed, key: &str) -> bool {
        let key_lower = key.to_lowercase();

        // Alias match
        if let Some(alias) = &f.alias {
            if alias.to_lowercase() == key_lower {
                return true;
            }
        }

        // Title match
        if let Some(title) = &f.title {
            if title.to_lowercase() == key_lower {
                return true;
            }
        }

        // Fallback: exact match on id or url
        if f.id == key || f.url == key {
            return true;
        }

        false
    }

    /// Find feed index by alias/title/id/url
    pub fn find_feed_index(&self, key: &str) -> Option<usize> {
        self.feeds.iter().enumerate().find_map(|(i, f)| {
            if Self::feed_matches(f, key) {
                Some(i)
            } else {
                None
            }
        })
    }

    /// Add feed (error if duplicate by id or url)
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

    /// Remove a feed & all its items using alias/title/id/url
    pub fn remove_feed(&mut self, key: &str) -> usize {
        let mut removed_ids: Vec<String> = Vec::new();

        self.feeds.retain(|f| {
            let to_remove = Self::feed_matches(f, key);
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
