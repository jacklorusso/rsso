## rsso

`rsso` is a minimal RSS feed organiser for the command line.

Keep up to date with feeds that you care about, or just find something
to read over coffee, in a distraction-free environment.

## Install

```bash
cargo install rsso
```

Or from source:

```bash
git clone https://github.com/jacklorusso/rsso
cd rsso
cargo install --path .
```

## Features

Subscribe to feeds, and optionally provide an alias

```bash
rsso sub https://blog.rust-lang.org/feed.xml
rsso sub https://blog.rust-lang.org/feed.xml --alias rust
```

Unsubscribe

```bash
rsso unsub rust
```

List subscribed feeds

```bash
rsso list
```

Show latest items

```bash
rsso
```

To override the default, you can specify with `-n <desired-number>` or
change the default in your config file.

```bash
rsso -n 50
```

Show items for one feed:

```bash
rsso feed rust
rsso feed rust -n 10
```

Refresh feeds manually:

```bash
rsso refresh
rsso refresh rust
```

Text-based output plays nice with other tools. For example:

```bash
rsso feed rust | grep nightly
```

## Optional config file

Create `~/.config/rsso/config.toml` to override defaults:

```toml
default_limit = 20
refresh_age_mins = 60
new_line_between_items = false
max_history_per_feed = 200
```

### History retention

Control how much item history is kept *per feed*:

```toml
max_history_per_feed = 200
```

`rsso` trims older items whenever a feed is refreshed, to keep reads and writes to state fast.

**Important:** if you wanted to list a very large number of items from a single feed, for whatever reason (perhaps you are searching for something)...

```bash
rsso feed rust -n 500
```

but your config says:

```toml
max_history_per_feed = 200
```

you will still only see 200 items. You would need to override `max_history_per_feed` in your config file if you are looking to list more than that.

### State file

State is stored in a platform‑appropriate location:

-   Linux: `~/.local/share/rsso/state.json`
-   macOS: `~/Library/Application Support/rsso/state.json`
-   Windows: `%APPDATA%\rsso\state.json`

You can override this:

```toml
state_file = "/custom/path.json"
```
Be careful though! If you don't move your original state file to this location, or if you somehow delete this file, you'll be starting fresh.


------------------------------------------------------------------------

## TODO

-   [] OPML import/export
-   [] Tags / groups
  
------------------------------------------------------------------------



## License

MIT
