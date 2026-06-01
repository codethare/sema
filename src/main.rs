mod checkers;
mod config;
mod sink;

use std::fmt::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tracing_subscriber::EnvFilter;

use checkers::{Checker, CheckerError, Severity};
use config::Config;
use sink::{Sink, send_notification};

#[derive(Parser)]
#[command(
    name = "sema",
    version,
    about = "Linux system resource monitor",
    after_help = "Config: ~/.config/sema/config.toml\nLog:    ~/.local/share/sema/sema.log"
)]
struct Cli {
    /// Print system status and exit (no notifications)
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Path to config file
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    /// Generate default config file and exit
    #[arg(long = "init")]
    init: bool,

    /// Overwrite existing config with --init
    #[arg(long = "force")]
    force: bool,

    /// Send a test notification to verify notifications work
    #[arg(long = "test")]
    test: bool,

    /// Generate shell completion script
    #[arg(long = "completions", value_enum)]
    completions: Option<Shell>,
}

static HUP_RECEIVED: AtomicBool = AtomicBool::new(false);
static TERM_RECEIVED: AtomicBool = AtomicBool::new(false);

fn handle_sighup() {
    HUP_RECEIVED.store(true, Ordering::SeqCst);
}

fn handle_sigterm() {
    TERM_RECEIVED.store(true, Ordering::SeqCst);
}

fn sd_notify(state: &str) {
    use std::os::unix::net::UnixDatagram;
    static SOCK: LazyLock<Option<UnixDatagram>> = LazyLock::new(|| UnixDatagram::unbound().ok());
    static PATH: LazyLock<Option<String>> = LazyLock::new(|| std::env::var("NOTIFY_SOCKET").ok());
    let sock = match SOCK.as_ref() {
        Some(s) => s,
        None => {
            tracing::warn!("sd_notify: no UnixDatagram socket available");
            return;
        }
    };
    let path = match PATH.as_ref() {
        Some(p) => {
            // Only accept abstract sockets (@...) or paths under /run/
            if !p.starts_with('@') && !p.starts_with("/run/") {
                tracing::warn!("sd_notify: invalid NOTIFY_SOCKET '{p}', ignoring");
                return;
            }
            p
        }
        None => {
            tracing::warn!("sd_notify: NOTIFY_SOCKET not set");
            return;
        }
    };
    let pid = std::process::id();
    let msg = format!("{state}\nMAINPID={pid}");
    if let Err(e) = sock.send_to(msg.as_bytes(), path) {
        tracing::warn!("sd_notify failed: {e}");
    }
}

struct Monitor {
    sys: System,
    sink: Sink,
    checkers: Vec<Box<dyn Checker>>,
    check_interval: Duration,
    config_path: Option<std::path::PathBuf>,
}

impl Monitor {
    fn new(cfg: Config, dry_run: bool, config_path: Option<std::path::PathBuf>) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        let check_interval = Duration::from_secs(cfg.check_interval_secs);
        let log_enabled = cfg.log.enabled;
        let checkers = cfg.into_checkers();
        let sink = Sink::new(dry_run, log_enabled);
        Self {
            sys,
            sink,
            checkers,
            check_interval,
            config_path,
        }
    }

    fn reload(&mut self) {
        let cfg = Config::load_from(self.config_path.as_deref());
        let log_enabled = cfg.log.enabled;
        self.check_interval = Duration::from_secs(cfg.check_interval_secs);
        self.checkers = cfg.into_checkers();
        self.sink.set_log_enabled(log_enabled);
        tracing::info!("config reloaded ({} checkers)", self.checkers.len());
    }

    fn print_banner(&self) {
        let mode = if self.sink.dry_run { " (dry-run)" } else { "" };
        let config_display = self
            .config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| config::config_path().display().to_string());
        println!(
            "🔍 sema {version}{mode}\n\
             ──────────────────────────────────\n\
             config:  {config_display}\n\
             log:     {log_path}\n\
             interval: {interval}s\n",
            version = env!("CARGO_PKG_VERSION"),
            mode = mode,
            config_display = config_display,
            log_path = Sink::log_path_display(),
            interval = self.check_interval.as_secs(),
        );
    }

    fn run(&mut self) {
        self.print_banner();
        sd_notify("READY=1\nSTATUS=Monitoring...");

        loop {
            if TERM_RECEIVED.load(Ordering::SeqCst) {
                println!("\nsema shutting down");
                sd_notify("STOPPING=1\nSTATUS=Shutting down...");
                break;
            }

            if HUP_RECEIVED.swap(false, Ordering::SeqCst) {
                self.reload();
                sd_notify("RELOADING=1\nSTATUS=Config reloaded");
                sd_notify("READY=1\nSTATUS=Monitoring...");
            } else {
                sd_notify("WATCHDOG=1\nSTATUS=Monitoring...");
            }

            // Refresh system data before running checks
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory();

            // Phase 1: run all checks, collect alerts
            struct PendingAlert<'a> {
                key: &'a str,
                cooldown: u64,
                alert: checkers::Alert,
            }

            let sys = &self.sys;
            let sink = &mut self.sink;
            let checkers = std::mem::take(&mut self.checkers);

            let mut pending: Vec<PendingAlert> = Vec::new();
            let mut crashed_checkers: Vec<&str> = Vec::new();
            let mut keep_checkers: Vec<Box<dyn Checker>> = Vec::new();

            thread::scope(|s| {
                let mut handles = Vec::with_capacity(checkers.len());
                for c in checkers {
                    let key = c.key();
                    let cooldown = c.cooldown_secs();
                    handles.push(s.spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| c.check(sys)));
                        (c, key, cooldown, result)
                    }));
                }
                for handle in handles {
                    // All spawned threads have joined once scope exits this block;
                    // unwrap is safe because catch_unwind inside prevents thread panic.
                    let (c, key, cooldown, result) = handle.join().unwrap();
                    match result {
                        Ok(Ok(Some(alert))) => {
                            sink.note_active(key, true);
                            pending.push(PendingAlert { key, cooldown, alert });
                            keep_checkers.push(c);
                        }
                        Ok(Ok(None)) => {
                            if sink.note_active(key, false) {
                                pending.push(PendingAlert {
                                    key,
                                    cooldown: 0,
                                    alert: checkers::Alert {
                                        severity: Severity::Warning,
                                        summary: format!("✅ {key} back to normal"),
                                        body: String::new(),
                                    },
                                });
                            }
                            keep_checkers.push(c);
                        }
                        Ok(Err(CheckerError::Io(e))) => {
                            tracing::error!("checker '{key}' I/O error: {e}");
                            crashed_checkers.push(key);
                            // c dropped — checker removed from rotation
                        }
                        Err(panic) => {
                            let msg = if let Some(s) = panic.as_ref().downcast_ref::<&str>() {
                                s
                            } else if let Some(s) = panic.as_ref().downcast_ref::<String>() {
                                s.as_str()
                            } else {
                                "<unknown>"
                            };
                            tracing::error!("checker '{key}' panicked: {msg}");
                            crashed_checkers.push(key);
                            // c dropped — checker removed from rotation
                        }
                    }
                }
            });
            self.checkers = keep_checkers;

            for k in crashed_checkers {
                sink.notify(
                    "checker_crash",
                    300,
                    &checkers::Alert {
                        severity: Severity::Critical,
                        summary: "⚠️ sema: checker crashed".to_string(),
                        body: format!("'{k}' crashed, sema is still running"),
                    },
                );
            }

            // Phase 2: send grouped or individual notifications
            if pending.len() == 1 {
                let p = &pending[0];
                if p.cooldown == 0 {
                    // Recovery notification — use separate cooldown tracking to
                    // prevent oscillation spam (alert → recovery → alert → ...)
                    sink.notify_recovery(p.key, &p.alert);
                } else {
                    sink.notify(p.key, p.cooldown, &p.alert);
                }
            } else if !pending.is_empty() {
                let min_cooldown = pending.iter().map(|p| p.cooldown).min().unwrap_or(60);
                let mut body = String::new();
                for (i, p) in pending.iter().enumerate() {
                    if i > 0 {
                        body.push_str(" | ");
                    }
                    let _ = write!(body, "{}: {}", p.alert.summary, p.alert.body);
                }
                let composite = checkers::Alert {
                    severity: Severity::Warning,
                    summary: format!("{} alerts", pending.len()),
                    body,
                };
                sink.notify("group", min_cooldown, &composite);
            }
            sleep(self.check_interval);
        }
    }

    fn dry_run(&mut self) {
        self.print_banner();
        println!("System state:\n");
        for c in &self.checkers {
            println!("{}", c.report(&self.sys));
        }
        println!();
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .init();
}

fn main() {
    init_tracing();

    let cli = Cli::parse();

    // Enforce single instance via PID file
    if !cli.init
        && !cli.test
        && let Err(e) = single_instance::SingleInstance::new("sema")
    {
        eprintln!("Error: another sema instance is already running ({e})");
        std::process::exit(1);
    }

    if cli.init {
        let path = cli.config.clone().unwrap_or_else(config::config_path);
        if path.exists() && !cli.force {
            eprintln!("Error: config already exists at {}\nUse --force to overwrite", path.display());
            std::process::exit(1);
        }
        match config::generate_default_config(&path) {
            Ok(()) => println!("Default config written to {}\nEdit it and run sema", path.display()),
            Err(e) => eprintln!("Error: cannot write config to {}: {e}", path.display()),
        }
        return;
    }

    if cli.test {
        let ok = send_notification(
            "🔍 sema test notification",
            "If you can read this, desktop notifications are working correctly.",
            Severity::Warning,
        );
        if ok {
            println!("Test notification sent successfully.");
        } else {
            eprintln!("Error: failed to send test notification. Is a notification daemon running?");
            std::process::exit(1);
        }
        return;
    }

    if let Some(shell) = cli.completions {
        generate(shell, &mut Cli::command(), "sema", &mut std::io::stdout());
        return;
    }

    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGHUP, handle_sighup) } {
        tracing::warn!("cannot register SIGHUP handler: {e}");
    }
    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGTERM, handle_sigterm) } {
        tracing::warn!("cannot register SIGTERM handler: {e}");
    }
    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGINT, handle_sigterm) } {
        tracing::warn!("cannot register SIGINT handler: {e}");
    }

    let dry_run = cli.dry_run;

    let config_path = cli.config;

    let cfg = match &config_path {
        Some(p) => Config::load_from(Some(p)),
        None => Config::load(),
    };
    let mut monitor = Monitor::new(cfg, dry_run, config_path);

    if dry_run {
        monitor.dry_run();
    } else {
        monitor.run();
    }
}
