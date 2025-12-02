use crate::state::{Feed, Item};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use feed_rs::parser;
use reqwest::blocking::Client;

/// Fetch and parse a feed.
/// Returns (title, Vec<Item>) on success.
pub fn fetch_feed(feed: &Feed) -> Result<(Option<String>, Vec<Item>)> {
    let client = Client::builder()
        .user_agent("rsso/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client.get(&feed.url).send()?;
    if !resp.status().is_success() {
        return Err(anyhow!("HTTP error {}", resp.status()));
    }

    let bytes = resp.bytes()?;
    let parsed = parser::parse(&bytes[..])?;

    // Extract feed title if present
    let feed_title = parsed.title.map(|t| t.content);

    let mut items = Vec::new();

    for entry in parsed.entries {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "(no title)".to_string());

        let link = entry
            .links
            .get(0)
            .map(|l| l.href.clone())
            .unwrap_or_else(|| "".to_string());

        let published_at = entry.published.map(|d| DateTime::<Utc>::from(d));

        let summary = entry.summary.as_ref().map(|s| s.content.clone());

        let item = Item {
            feed_id: feed.id.clone(),
            title,
            link,
            summary,
            published_at,
            first_seen_at: Utc::now(),
        };

        items.push(item);
    }

    Ok((feed_title, items))
}
