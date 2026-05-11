use std::time::Instant;

use sysinfo::{Networks, System};

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Network {
    cfg: MetricConfig,
    networks: Networks,
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Option<Instant>,
}

impl Network {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            networks: Networks::new_with_refreshed_list(),
            prev_rx: 0, prev_tx: 0, prev_time: None,
        }
    }
}

impl Checker for Network {
    fn key(&self) -> &'static str {
        "network"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &System) -> Option<Alert> {
        self.networks.refresh(false);
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in &self.networks {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }

        let now = Instant::now();
        let elapsed = match self.prev_time {
            Some(t) => now.duration_since(t),
            None => {
                self.prev_rx = total_rx;
                self.prev_tx = total_tx;
                self.prev_time = Some(now);
                return None;
            }
        };

        let rx_delta = total_rx.saturating_sub(self.prev_rx);
        let tx_delta = total_tx.saturating_sub(self.prev_tx);
        self.prev_rx = total_rx;
        self.prev_tx = total_tx;
        self.prev_time = Some(now);

        let secs = elapsed.as_secs_f64().max(0.001);
        let rx_mbps = rx_delta as f64 / secs / (1024.0 * 1024.0);
        let tx_mbps = tx_delta as f64 / secs / (1024.0 * 1024.0);
        let total_mbps = rx_mbps + tx_mbps;

        if total_mbps < self.cfg.threshold {
            return None;
        }
        Some(Alert {
            summary: format!("{} Network traffic high", self.cfg.severity_label(total_mbps, false)),
            body: format!("↓ {rx_mbps:.1} ↑ {tx_mbps:.1} MB/s (threshold: {thr} MB/s)",
                thr = self.cfg.threshold),
        })
    }

    fn report(&self, _sys: &System) -> String {
        if self.networks.iter().count() == 0 {
            return "  Network N/A".into();
        }
        let mut parts: Vec<String> = Vec::new();
        for (name, data) in &self.networks {
            let rx = data.total_received();
            let tx = data.total_transmitted();
            parts.push(format!("{name} ↓{} ↑{}", fmt_bytes(rx as f64), fmt_bytes(tx as f64)));
        }
        format!("  Network {}", parts.join("  "))
    }
}

fn fmt_bytes(b: f64) -> String {
    if b > 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1}GiB", b / (1024.0 * 1024.0 * 1024.0))
    } else if b > 1024.0 * 1024.0 {
        format!("{:.1}MiB", b / (1024.0 * 1024.0))
    } else if b > 1024.0 {
        format!("{:.1}KiB", b / 1024.0)
    } else {
        format!("{b}B")
    }
}
