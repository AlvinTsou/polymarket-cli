# PMCC Deployment Guide — macOS launchd

## Services

| Service | Label | Purpose | Port |
|---------|-------|---------|------|
| Dashboard | `com.pmcc.dashboard` | Live web dashboard with auto-refresh | `localhost:3456` |
| Monitor | `com.pmcc.monitor` | Real-time scan + condition triggers + paper trading | — |

Both services:
- Auto-start on login (`RunAtLoad`)
- Auto-restart on crash (`KeepAlive`)
- Log to `~/.config/polymarket/smart/`

## Plist Locations

```
~/Library/LaunchAgents/com.pmcc.dashboard.plist
~/Library/LaunchAgents/com.pmcc.monitor.plist
```

## Setup

### Dashboard

```bash
# Load and start (immediate)
launchctl load ~/Library/LaunchAgents/com.pmcc.dashboard.plist

# Verify
curl -s -o /dev/null -w "%{http_code}" http://localhost:3456
# Should return 200

# Open in browser
open http://localhost:3456
```

### Monitor

Monitor requires a saved config before the launchd service can use `--load`:

```bash
# 1. Configure and save (run once manually)
polymarket smart monitor \
  --interval 3m \
  --min-confidence med \
  --min-wallets 1 \
  --notify \
  --paper-trade \
  --amount 10 \
  --max-per-day 50 \
  --save

# 2. Ctrl+C to stop the manual run

# 3. Load the launchd service (uses --load to read saved config)
launchctl load ~/Library/LaunchAgents/com.pmcc.monitor.plist
```

## Management Commands

```bash
# Start / Stop
launchctl start com.pmcc.dashboard
launchctl stop com.pmcc.dashboard
launchctl start com.pmcc.monitor
launchctl stop com.pmcc.monitor

# Unload (remove from launchd, won't auto-start)
launchctl unload ~/Library/LaunchAgents/com.pmcc.dashboard.plist
launchctl unload ~/Library/LaunchAgents/com.pmcc.monitor.plist

# Reload (after binary rebuild)
launchctl stop com.pmcc.dashboard && launchctl start com.pmcc.dashboard
launchctl stop com.pmcc.monitor && launchctl start com.pmcc.monitor

# Check status
launchctl list | grep pmcc
```

## Logs

```bash
# Dashboard log
tail -f ~/.config/polymarket/smart/dashboard.log

# Monitor log
tail -f ~/.config/polymarket/smart/monitor.log

# Clear logs
> ~/.config/polymarket/smart/dashboard.log
> ~/.config/polymarket/smart/monitor.log
```

## After Rebuild

When you rebuild the release binary (`cargo build --release`), restart the services:

```bash
launchctl stop com.pmcc.dashboard && launchctl start com.pmcc.dashboard
launchctl stop com.pmcc.monitor && launchctl start com.pmcc.monitor
```

## Update Monitor Config

```bash
# Stop the service
launchctl stop com.pmcc.monitor

# Run with new settings and save
polymarket smart monitor \
  --interval 1m \
  --min-wallets 2 \
  --market-include "election,AI,crypto" \
  --market-exclude "sports" \
  --odds-threshold 5.0 \
  --paper-trade \
  --amount 20 \
  --max-per-day 100 \
  --notify \
  --save

# Ctrl+C, then restart
launchctl start com.pmcc.monitor
```

## Troubleshooting

```bash
# Check if service is running
launchctl list | grep pmcc
# PID column: number = running, 0 = stopped, - = not loaded

# Check exit code
launchctl list com.pmcc.dashboard
# "LastExitStatus" = 0 means clean exit

# Port already in use
lsof -i :3456
# Kill the process if needed, then restart

# Service won't start
launchctl unload ~/Library/LaunchAgents/com.pmcc.dashboard.plist
launchctl load ~/Library/LaunchAgents/com.pmcc.dashboard.plist
```

## Config File Location

```
~/.config/polymarket/smart/monitor.json   # Monitor config (--save/--load)
~/.config/polymarket/smart/telegram.json  # Telegram bot config
~/.config/polymarket/smart/wallets.json   # Watched wallets
~/.config/polymarket/smart/follows.jsonl  # Follow trade records
~/.config/polymarket/smart/signals.jsonl  # Signal history
```
