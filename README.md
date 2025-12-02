## rsso

`rsso` is a minimal RSS feed organiser for the command line.
I'm building it because I want it, and I want to learn Rust over the Summer.

This is a work in progress.

## Features

- Subscribe to feeds, with or without using an alias

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

Show latest items (default: 20, or config-defined)

```bash
rsso
```
Override default / config-defined number of items

```bash
rsso -n 10
```

Show items for one feed (instead of all subscribed feeds)

```bash
rsso feed rust -n 5
```

Refresh feeds manually

```bash
rsso refresh
rsso refresh rust
```

Text-based output plays nice with other tools - for example, you can search for key words with `grep`

```bash
rsso feed rust | grep nightly 

```

## Config

Create `~/.config/rsso/config.toml`

```toml
default_limit = 20
refresh_age_mins = 60
# state_file = "/custom/path.json"   # optional override
```

## Install

Probably not ready for consumption just yet, but I'm shipping as I go... You can install it if you want!

```
cargo install rsso
```

Or from source:
```bash
git clone https://github.com/jacklorusso/rsso
cd rsso
cargo install --path .
```

## License

MIT
