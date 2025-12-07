## rsso

`rsso` is a minimal RSS feed organiser for the command line.

Keep up to date with feeds that you care about, or just find something to read over coffee, in a distraction-free environment.

## Install

```
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

To override the default, you can specify with `-n <desired-number>` or change the default in your [config file]()

```bash
rsso -n 50
```

Show items for one feed (instead of all subscribed feeds)

```bash
rsso feed rust
rsso feed rust -n 10
```

Import feeds from an OPML file (feeds are added alongside existing ones):

```bash
rsso import ~/Downloads/subscriptions.opml
```

Export current subscriptions to an OPML file (defaults next to your state file, or specify a path):

```bash
rsso export
rsso export --output ./my_feeds.opml
```

Refresh feeds manually (items are cached for one hour by default, but you can change this in your [config file]())

```bash
rsso refresh
rsso refresh rust
```

Text-based output plays nice with other tools - for example, you can search for key words with `grep`

```bash
rsso feed rust | grep nightly 

```

## Optional config file

To override `rsso`'s defaults, create a `~/.config/rsso/config.toml` file.

```toml
default_limit = 20
refresh_age_mins = 60
new_line_between_items = false
```

### State file

Your `rsso` data state is kept in a JSON file in a path that should make sense for your OS, as defined by [dirs-rs](https://codeberg.org/dirs/dirs-rs):

- Linux: `~/.local/share/rsso/state.json`
- macOS: `~/Library/Application Support/rsso/state.json`
- Windows: `%APPDATA%\\rsso\\state.json`

You don't need to touch this, but you _can_ override the location by setting in your `config.toml` if you wish.
```toml
state_file = "/custom/path.json"
```
Be careful though! If you don't move your original state file to this location, or if you somehow delete this file, you'll be starting fresh.

## TODO:

-[] Tags / groups

## License

MIT
