# sema (σῆμα)

> **σῆμα** (sêma) — Ancient Greek for "signal, alarm".

A lightweight system resource monitoring daemon for Linux. It monitors CPU, memory, swap, battery, and time, and sends desktop notifications via `notify-send` when configured thresholds are exceeded.

[**中文文档 (Chinese)**](README.zh.md)

## Features

| Metric  | Default Threshold | Description |
|---------|-------------------|-------------|
| CPU     | > 50%             | Global CPU usage exceeds threshold |
| Memory  | > 50%             | Physical memory usage exceeds threshold |
| Swap    | > 80%             | Swap usage exceeds threshold |
| Battery | < 84%             | Battery charge drops below threshold |
| Time    | :00 / :30         | Chime on the hour and half-hour |

- Per-metric notification cooldown — no spam
- Fully configurable via TOML file
- Logs all alerts to disk
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

Download the pre-built binary from the [Releases](https://github.com/codethare/sema/releases) page:

```bash
chmod +x sema
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

### Autostart

For i3/sway, add to `~/.config/sway/config`:

```bash
exec --no-startup-id sema
```

### Systemd user service

Create `~/.config/systemd/user/sema.service`:

```ini
[Unit]
Description=sema System Monitor
After=graphical-session.target

[Service]
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
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `check_interval_secs` | integer | 10 | Global check interval (seconds) |
| `[metric].enabled` | boolean | true | Enable/disable the metric |
| `[metric].threshold` | float | see table | Alert threshold (%) |
| `[metric].cooldown_secs` | integer | 60 | Per-metric notification cooldown (seconds) |

> `[metric]` can be `cpu`, `memory`, `swap`, or `battery`.
> `[time]` has no `threshold` field.

### Disabling a metric

```toml
[cpu]
enabled = false
```

## Logging

All triggered alerts are recorded to:

```
~/.local/share/sema/sema.log
```

Format:

```
[2026-05-08 12:34:56] ⚠️ CPU 负载过高 | 当前 CPU 使用率: 95.0%（阈值: 50.0%）
```

Tail the log:

```bash
tail -f ~/.local/share/sema/sema.log
```

## Crates used

| Crate | Purpose | Version |
|-------|---------|---------|
| [sysinfo](https://crates.io/crates/sysinfo) | CPU / memory / swap info | 0.33 |
| [notify-rust](https://crates.io/crates/notify-rust) | Desktop notifications (notify-send) | 4.11 |
| [chrono](https://crates.io/crates/chrono) | Time handling | 0.4 |
| [serde](https://crates.io/crates/serde) | Config serialization | 1 |
| [toml](https://crates.io/crates/toml) | TOML config parsing | 0.8 |

Battery info is read directly from the Linux kernel sysfs (`/sys/class/power_supply/BAT*/capacity`) — zero extra dependencies.

## License

MIT
