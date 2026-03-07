# Polymarket CLI — Project Context

## Overview

Rust CLI for Polymarket prediction market trading + **Daily Article Digest** feature.
Binary name: `polymarket`. Edition 2024, Rust 1.88.0+.

## Build & Test

```bash
cargo build                      # Build
cargo test                       # Run all tests (unit + integration)
cargo clippy -- -D warnings      # Lint (must pass with zero warnings)
cargo fmt --check                # Format check
```

All four checks must pass before committing.

## Architecture

```
src/
├── main.rs          # CLI entry (clap Parser, Commands enum, run() dispatch)
├── auth.rs          # Wallet/signer resolution, RPC providers
├── config.rs        # Config at ~/.config/polymarket/config.json
├── shell.rs         # Interactive REPL
├── storage.rs       # SQLite storage (articles.db) — WAL mode
├── gemini.rs        # Google Gemini API client for article summarization
├── notify.rs        # Telegram + Email notification dispatch
├── commands/        # One file per command group (clap Args + Subcommand)
│   ├── mod.rs
│   ├── article.rs   # article add/list/get/delete/summarize
│   ├── digest.rs    # digest send/preview/setup
│   ├── bot.rs       # bot start (Telegram long-polling)
│   ├── markets.rs, events.rs, clob.rs, ...  # Trading commands
│   └── wallet.rs, setup.rs, upgrade.rs      # System commands
└── output/          # Dual rendering (Table + JSON) per command group
    ├── mod.rs       # OutputFormat enum, truncate(), format_decimal(), print_json(), detail_field! macro
    ├── article.rs   # Article table/detail rendering
    ├── digest.rs    # Digest preview formatting
    └── markets.rs, events.rs, ...
```

## Conventions

### Adding a new command group

1. Create `src/commands/newcmd.rs` with `NewCmdArgs` (clap `Args`) and `NewCmdCommand` (clap `Subcommand`)
2. Create `src/output/newcmd.rs` with table rendering functions
3. Register in `src/commands/mod.rs` and `src/output/mod.rs`
4. Add variant to `Commands` enum in `src/main.rs` and match arm in `run()`

### Code patterns

- All fallible functions return `anyhow::Result<()>`
- Every command supports dual output via `OutputFormat` (Table/Json)
- Table output uses `tabled` crate with `Style::rounded()`
- Detail views use `print_detail_table(rows)` with `detail_field!` macro
- JSON output uses `crate::output::print_json(&data)`
- Config fields for optional features use `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`

### Config

File: `~/.config/polymarket/config.json` (mode 0o600)

Core fields: `private_key`, `chain_id`, `signature_type`
Digest fields (all optional): `gemini_api_key`, `telegram_bot_token`, `telegram_chat_id`, `smtp_host`, `smtp_username`, `smtp_password`, `email_from`, `email_to`

Use `config::load_config()` (returns Option), `config::load_config_or_default()`, or `config::save_config(&cfg)`.

### Storage (SQLite)

File: `~/.config/polymarket/articles.db`

```sql
articles(id, url UNIQUE, title, raw_content, summary, source, added_at, summarized_at)
```

Use `storage::open_db()` to get a connection. All CRUD functions are in `src/storage.rs`.

## Daily Article Digest — Feature Summary

### Commands

```
polymarket article add <url>            # Fetch + extract + store + summarize
polymarket article list [--limit N]     # List stored articles
polymarket article get <id>             # Show detail + summary
polymarket article delete <id>          # Remove
polymarket article summarize [--id N]   # Re-summarize via Gemini

polymarket digest send [--channel telegram|email|all] [--since YYYY-MM-DD]
polymarket digest preview [--since YYYY-MM-DD]
polymarket digest setup                 # Interactive config wizard

polymarket bot start                    # Telegram bot (long-polling)
```

### Data flow

```
URL → reqwest fetch → scraper extract → SQLite store → Gemini summarize → SQLite update
                                                                              ↓
                           cron: polymarket digest send → notify (TG/Email)
```

### Key dependencies for digest feature

- `rusqlite` (bundled SQLite)
- `teloxide` (Telegram bot framework)
- `lettre` (email, default-features=false to avoid native-tls conflict)
- `reqwest` (HTTP client with json feature)
- `scraper` (HTML content extraction)
- `url` (URL validation)

### Telegram bot

- Runs via `polymarket bot start` (foreground, long-polling)
- Handles: URL messages → save+summarize, `/start`, `/list`, `/digest`
- Uses `teloxide::repl` for message dispatch

### Cron setup

```
0 8 * * * polymarket digest send --channel all >> ~/.polymarket-digest.log 2>&1
```

## Known Limitations / Future Work

- Content extraction uses CSS selectors (`article`, `main`, `body` fallback) — may not work well on JavaScript-heavy SPAs
- No deduplication by content (only by URL)
- Telegram bot has no auth — anyone who knows the bot token can send articles
- No pagination for `article list`
- Gemini API key stored in plaintext in config file
- `readability` crate was not used (compatibility issues with edition 2024); `scraper` is used instead

## Git

- Main branch: `master` / `main`
- Feature branch: `claude/daily-article-digest-xalIX`
- CI: GitHub Actions — fmt, clippy, test on Ubuntu + macOS
- Release: tag-triggered, cross-platform builds with checksums
