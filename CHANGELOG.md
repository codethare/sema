# Changelog

## [0.0.9] — 2026-05-13

### Bug Fixes
- `MetricConfig::default()` threshold was 0.0, causing false alerts when config section missing `threshold` field (P0)
- `send_notification()` failure was completely silent, now logs `tracing::warn!` (P0)
- `sd_notify()` errors were silently discarded, now logs socket/connection failures (P1)
- Checker crash notifications bypassed `Sink` cooldown, causing notification spam (P1)

### Added
- Startup check for `notify-send` availability with warning log

## [0.0.8] — 2026-05-12

### Code Quality
- Emoji rendering centralized into `Severity::emoji()` enum method
- `MetricConfig::severity_label()` → `severity()` returning `Severity` enum
- `Checker::check()` now returns `Result<Option<Alert>, CheckerError>` instead of `Option<Alert>`
- `ConfigRaw` renamed to `ConfigFile`, merged via `From` trait
- Replaced `notify-rust` (zbus/D-Bus) with direct `notify-send` invocation
  - Binary size reduced by 41% (3.4 MB → 2.0 MB)
  - Compile time reduced ~2.7x
  - Notification urgency set automatically based on alert severity

### Bug Fixes
- Battery checker now scans `/sys/class/power_supply/` for all batteries instead of hardcoded BAT0-BAT3

### Build & CI
- Multi-architecture release matrix: x86_64, aarch64, armv7, i686 (all musl)
- Cargo caching across builds
- Auto-generated release notes

## [0.0.7] — 2026-05

- README completions docs, version bump
- 9 integration tests, clap completions support
- `fmt_bytes`/`fmt::Write` cleanup

## [0.0.6]

- Memory zero-guard, battery u16 type
- Removed TOP3 process display
- Load average display
- `--test` flag for notification verification
- Single-instance enforcement via PID file
- Per-interface network report, code cleanup

## [0.0.5]

- README fixes, `--force` docs
- clap CLI parsing with `--config` error handling
- `--init --force` overwrite support
- Network report, `cfg!` platform guards
- English duration text

## [0.0.4]

- Fix process refresh, temperature safety patterns
- Process listing in CPU/memory notifications

## [0.0.3]

- 13 improvements:
  - Integration tests (dry-run, --version, --help)
  - Graceful shutdown (SIGTERM/SIGINT)
  - Config validation (clamp thresholds/cooldown/interval)
  - `--config PATH` custom config file
  - `--init` generate default config
  - Recovery notification when metric returns to normal
  - Multi-level thresholds (warning + critical)
  - Notification grouping (composite alerts)
  - Duration tracking preserved across SIGHUP reload

## [0.0.2]

- English UI, CPU-only temperature filtering
- Log toggle, crash notification
- Panic recovery via catch_unwind

## [0.0.1]

- Initial release
- CPU, memory, swap, battery monitoring
- Desktop notifications via notify-rust
- TOML configuration
- SIGHUP hot reload
- sd_notify systemd integration
- Log rotation