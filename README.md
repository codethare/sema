# sema (σῆμα)

> **σῆμα** (sêma) — Ancient Greek for "signal, alarm".

A lightweight system resource monitoring daemon for Linux. It monitors CPU, memory, swap, battery, temperature, and network, and sends desktop notifications via `notify-send` when configured thresholds are exceeded.

[**中文文档 (Chinese)**](README.zh.md)

## Features

| Metric      | Default Threshold | Description |
|-------------|-------------------|-------------|
| CPU         | > 50%             | Global CPU usage. Shows top process. |
| Memory      | > 50%             | Physical memory usage. Shows top process. |
| Swap        | > 80%             | Swap usage. |
| Battery     | < 84%             | Battery charge level. |
| Temperature | > 80°C            | CPU/GPU/NVMe component temperature. Shows hottest sensor. |
| Network     | > 100 MB/s        | Total network throughput (RX + TX). |
| Time        | :00 / :30         | Chime on the hour and half-hour. |

- Per-metric notification cooldown — no spam
- Notification shows **duration** the condition has persisted (e.g. "持续 5m30s")
- Notification includes the **top resource-consuming process** (CPU/memory alerts)
- Fully configurable via TOML file
- Logs all alerts to disk with **auto-rotation** (5 MB)
- **SIGHUP hot reload** — reload config without restarting
- **sd_notify** support — integrates with systemd watchdog
- Respects XDG directory standards

## Installation

### From source

```bash
git clone https://github.com/codethare/sema.git
cd sema
cargo build --release
sudo cp target/release/sema /usr/local/bin/
```

### From a release

Download the latest archive from the [Releases](https://github.com/codethare/sema/releases) page:

```bash
# Download and verify
curl -OL https://github.com/codethare/sema/releases/latest/download/sema-x86_64-linux-musl.tar.gz
curl -OL https://github.com/codethare/sema/releases/latest/download/sema-x86_64-linux-musl.tar.gz.sha256

# Verify checksum
sha256sum --check sema-x86_64-linux-musl.tar.gz.sha256

# Extract and install
tar xzf sema-x86_64-linux-musl.tar.gz
sudo mv sema /usr/local/bin/
```

## Usage

Run as a daemon:

```bash
sema
```

Preview system status without sending notifications:

```bash
sema --dry-run    # or sema -n
```

Show help:

```bash
sema --help
```

Reload config without restart (send SIGHUP):

```bash
killall -HUP sema
```

### Autostart

For i3/sway, add to `~/.config/sway/config`:

```bash
exec --no-startup-id sema
```

### Systemd user service

sema supports the `sd_notify` protocol — `systemctl status` shows live status:

```ini
# ~/.config/systemd/user/sema.service
[Unit]
Description=sema System Monitor
After=graphical-session.target

[Service]
Type=notify
ExecStart=/usr/local/bin/sema
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Enable it:

```bash
systemctl --user enable --now sema
```

With `Type=notify`, systemd waits for sema to signal readiness before marking the service as active, and sema sends periodic `WATCHDOG=1` updates.

## Configuration

Config file: `~/.config/sema/config.toml` (or `$XDG_CONFIG_HOME/sema/config.toml`).

All fields have sensible defaults. You only need to specify what you want to change.

### Full example

```toml
# Global check interval in seconds (default: 10)
check_interval_secs = 10

[cpu]
enabled = true
threshold = 50.0
cooldown_secs = 60

[memory]
enabled = true
threshold = 50.0
cooldown_secs = 60

[swap]
enabled = true
threshold = 80.0
cooldown_secs = 60

[battery]
enabled = true
threshold = 84.0
cooldown_secs = 60

[time]
enabled = true
cooldown_secs = 60

[temperature]
enabled = true
threshold = 80.0
cooldown_secs = 60

[network]
enabled = true
threshold = 100.0
cooldown_secs = 60

[log]
enabled = true
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `check_interval_secs` | integer | 10 | Global check interval (seconds) |
| `[metric].enabled` | boolean | true | Enable/disable the metric |
| `[metric].threshold` | float | see table | Alert threshold (%) |
| `[metric].cooldown_secs` | integer | 60 | Per-metric notification cooldown (seconds) |
| `[log].enabled` | boolean | true | Enable/disable log file |

> `[metric]` can be `cpu`, `memory`, `swap`, `battery`, or `temperature`.
> `[time]` has no `threshold` field.
> `[network]` threshold is in **MB/s** (total throughput), not percentage.

### Disabling a metric

```toml
[cpu]
enabled = false
```

## Logging

All triggered alerts are recorded to the log file (can be disabled via `[log].enabled = false` in config):

```
~/.local/share/sema/sema.log
```

Format:

```
[2026-05-08 12:34:56] ⚠️ CPU overloaded | CPU usage: 95.0% (top: firefox 42.3%, threshold: 50.0%) (持续 5m30s)
```

The log **auto-rotates** at 5 MB — the old file is renamed to `sema.log.1` and a fresh log starts.

Tail the log:

```bash
tail -f ~/.local/share/sema/sema.log
```

## SIGHUP hot reload

When sema receives `SIGHUP`, it reloads the config file and recreates all checkers without restarting the process:

```bash
killall -HUP sema
```

This is useful for tweaking thresholds or enabling/disabling metrics on the fly.

## Crates used

| Crate | Purpose | Version |
|-------|---------|---------|
| [sysinfo](https://crates.io/crates/sysinfo) | CPU / memory / swap / processes / temperature | 0.33 |
| [notify-rust](https://crates.io/crates/notify-rust) | Desktop notifications (notify-send) | 4.11 |
| [chrono](https://crates.io/crates/chrono) | Time handling | 0.4 |
| [serde](https://crates.io/crates/serde) | Config serialization | 1 |
| [toml](https://crates.io/crates/toml) | TOML config parsing | 0.8 |
| [signal-hook](https://crates.io/crates/signal-hook) | SIGHUP handler | 0.3 |

Battery info is read directly from the Linux kernel sysfs (`/sys/class/power_supply/BAT*/capacity`) — zero extra dependencies.

## License

MIT
