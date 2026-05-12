# Contributing

Thanks for your interest in sema!

## Quick Start

```bash
# Build & test
cargo build
cargo test
cargo clippy -- -D warnings

# Run (dry-run mode, no notifications)
cargo run -- --dry-run

# Format code
cargo fmt
```

## Project Structure

```
src/
├── main.rs           — CLI entry, monitor loop
├── config.rs         — TOML config parsing, validation
├── sink.rs           — Notification dispatch, log, cooldown tracking
├── checkers/
│   ├── mod.rs        — Checker trait, Alert, CheckerError
│   ├── cpu.rs        — CPU usage checker
│   ├── memory.rs     — Memory usage checker
│   ├── swap.rs       — Swap usage checker
│   ├── battery.rs    — Battery capacity checker
│   ├── temperature.rs — CPU temperature checker
│   ├── network.rs    — Network throughput checker
│   └── time.rs       — Time reminder checker
tests/
└── sema.rs           — Integration tests
```

## Adding a New Checker

1. Create `src/checkers/your_checker.rs`
2. Implement the `Checker` trait
3. Register in `checkers/mod.rs::all_checkers()`
4. Add config section in `config.rs`
5. Add a test metric in the export table of `README.md`

## Pull Request

- Run `cargo test` and `cargo clippy` before submitting
- Update `CHANGELOG.md` for user-facing changes
- Keep PRs focused on a single concern

## License

MIT — see [LICENSE](LICENSE).