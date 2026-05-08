use sysinfo::{Networks, System};

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Network {
    cfg: MetricConfig,
    prev_rx: u64,
    prev_tx: u64,
}

impl Network {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg, prev_rx: 0, prev_tx: 0 }
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
        let networks = Networks::new_with_refreshed_list();
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in &networks {
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }

        if self.prev_rx == 0 {
            self.prev_rx = total_rx;
            self.prev_tx = total_tx;
            return None;
        }

        let rx_delta = total_rx.saturating_sub(self.prev_rx);
        let tx_delta = total_tx.saturating_sub(self.prev_tx);
        self.prev_rx = total_rx;
        self.prev_tx = total_tx;

        let interval = self.cfg.cooldown_secs.max(1) as f64;
        let rx_mbps = rx_delta as f64 / interval / (1024.0 * 1024.0);
        let tx_mbps = tx_delta as f64 / interval / (1024.0 * 1024.0);
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
        let networks = Networks::new_with_refreshed_list();
        let count = networks.iter().count();
        if count == 0 {
            return "  Network N/A".into();
        }
        let names: Vec<&str> = networks.keys().map(|n| n.as_str()).collect();
        format!("  Network {} interfaces: {}", count, names.join(", "))
    }
}
