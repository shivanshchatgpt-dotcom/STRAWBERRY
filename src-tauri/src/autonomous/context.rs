//! 🌍 SchedContext builder — real system signals for the Adaptive Scheduler.
//!
//! Zero new dependencies: reads `/proc/stat` + `/proc/meminfo` directly
//! (Linux). Missing files / parse failures degrade to "calm machine"
//! defaults — the scheduler stays functional, just less informed. This is
//! the graceful-degradation requirement in practice.
//!
//! CPU: two samples of `/proc/stat` aggregated across all cores; the first
//! call has no previous sample, so it returns 0.0 (calm) and caches.
//! Battery: `/sys/class/power_supply/*/status` + `capacity` when present;
//! desktops without batteries report full + on AC, which is the safe
//! default (never defers work for a battery that doesn't exist).
//!
//! All values are clamped 0.0–1.0.

use std::sync::Mutex;

use super::scheduler::SchedContext;

/// Cached delta state for CPU sampling.
#[derive(Default)]
struct CpuSample {
    idle_total: u64,
    total: u64,
    valid: bool,
}

/// Process-wide sampler. `Mutex` keeps it usable from any loop thread.
#[derive(Default)]
pub struct SystemProbe {
    cpu: Mutex<CpuSample>,
}

impl SystemProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read `/proc/stat` once. `None` on any parse failure.
    fn read_proc_stat() -> Option<(u64, u64)> {
        // cpu  user nice system idle iowait irq softirq steal guest guest_nice
        let first = std::fs::read_to_string("/proc/stat").ok()?;
        let line = first.lines().next()?;
        if !line.starts_with("cpu ") {
            return None;
        }
        let fields: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|f| f.parse::<u64>().ok())
            .collect();
        if fields.len() < 5 {
            return None;
        }
        let idle = fields[3] + fields.get(4).copied().unwrap_or(0); // idle + iowait
        let total: u64 = fields.iter().sum();
        Some((idle, total))
    }

    /// Sample CPU load 0.0–1.0. First call = 0.0 (needs two samples).
    pub fn cpu_load(&self) -> f32 {
        let (idle, total) = match Self::read_proc_stat() {
            Some(v) => v,
            None => return 0.0,
        };
        let mut s = self.cpu.lock().unwrap_or_else(|e| e.into_inner());
        if !s.valid || total <= s.total {
            s.idle_total = idle;
            s.total = total;
            s.valid = true;
            return 0.0;
        }
        let d_total = total - s.total;
        let d_idle = idle.saturating_sub(s.idle_total);
        s.idle_total = idle;
        s.total = total;
        if d_total == 0 {
            return 0.0;
        }
        1.0 - (d_idle as f32 / d_total as f32)
    }

    /// Memory pressure 0.0–1.0 from `/proc/meminfo`. Missing file → 0.0.
    pub fn mem_load(&self) -> f32 {
        let text = match std::fs::read_to_string("/proc/meminfo") {
            Ok(t) => t,
            Err(_) => return 0.0,
        };
        let mut mem_total: Option<u64> = None;
        let mut mem_available: Option<u64> = None;
        for line in text.lines() {
            if let Some(v) = line
                .strip_prefix("MemTotal:")
                .and_then(|r| r.trim().split_whitespace().next())
                .and_then(|n| n.parse::<u64>().ok())
            {
                mem_total = Some(v);
            } else if let Some(v) = line
                .strip_prefix("MemAvailable:")
                .and_then(|r| r.trim().split_whitespace().next())
                .and_then(|n| n.parse::<u64>().ok())
            {
                mem_available = Some(v);
            }
        }
        match (mem_total, mem_available) {
            (Some(t), Some(a)) if t > 0 => 1.0 - (a as f32 / t as f32),
            _ => 0.0,
        }
    }

    /// Battery 0.0–1.0 + on_ac. Defaults: full + AC when no battery exists.
    pub fn battery(&self) -> (f32, bool) {
        let dir = match std::fs::read_dir("/sys/class/power_supply") {
            Ok(d) => d,
            Err(_) => return (1.0, true),
        };
        for entry in dir.flatten() {
            let base = entry.path();
            let kind = std::fs::read_to_string(base.join("type")).unwrap_or_default();
            if !kind.trim().eq_ignore_ascii_case("battery") {
                continue;
            }
            let status = std::fs::read_to_string(base.join("status")).unwrap_or_default();
            let on_ac = status.trim().eq_ignore_ascii_case("charging")
                || status.trim().eq_ignore_ascii_case("full")
                || status.trim().eq_ignore_ascii_case("unknown");
            let cap = std::fs::read_to_string(base.join("capacity"))
                .ok()
                .and_then(|c| c.trim().parse::<f32>().ok())
                .unwrap_or(100.0);
            return (cap.clamp(0.0, 100.0) / 100.0, on_ac);
        }
        (1.0, true)
    }

    /// Heavy-work heuristic: CPU load ≥ 80%. Uses the DELTA between the
    /// two most recent samples (already smoothed by the sampler); a single
    /// 200 ms jiffy spike from parallel tests must not flip it — so we
    /// require a sustained reading by re-checking once.
    pub fn heavy_work(&self) -> bool {
        let first = self.cpu_load();
        if first < 0.8 {
            return false;
        }
        // Confirm with a second sample — only sustained load counts.
        self.cpu_load() >= 0.8
    }

    /// Build a full scheduler context snapshot from live system signals.
    /// `user_activity`, `idle_secs` and `new_events` come from the caller
    /// (the runtime knows those better than /proc does).
    pub fn context(&self, user_activity: f32, idle_secs: u64, new_events: u32) -> SchedContext {
        let (battery, on_ac) = self.battery();
        SchedContext {
            user_activity: user_activity.clamp(0.0, 1.0),
            cpu_load: self.cpu_load().clamp(0.0, 1.0),
            mem_load: self.mem_load().clamp(0.0, 1.0),
            battery,
            on_ac,
            heavy_work: self.heavy_work(),
            idle_secs,
            new_events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_builds_a_finite_context_everywhere() {
        // On any machine (CI containers, dev laptops) this must produce a
        // fully clamped, finite context — the graceful-degradation guarantee.
        let probe = SystemProbe::new();
        let ctx = probe.context(0.5, 60, 3);
        assert!((0.0..=1.0).contains(&ctx.cpu_load));
        assert!((0.0..=1.0).contains(&ctx.mem_load));
        assert!((0.0..=1.0).contains(&ctx.battery));
        assert!(ctx.cpu_load.is_finite() && ctx.mem_load.is_finite());
    }

    #[test]
    fn first_cpu_sample_is_calm_then_delta_moves() {
        let probe = SystemProbe::new();
        assert_eq!(probe.cpu_load(), 0.0, "first sample has no delta");
        // Second sample right after: jiffies barely move → still low.
        let second = probe.cpu_load();
        assert!((0.0..=1.0).contains(&second));
    }

    #[test]
    fn battery_defaults_to_full_ac_without_battery() {
        // Most dev machines/CI have no /sys/class/power_supply battery.
        let probe = SystemProbe::new();
        let (b, ac) = probe.battery();
        if std::path::Path::new("/sys/class/power_supply/BAT0").exists()
            || std::fs::read_dir("/sys/class/power_supply")
                .map(|d| {
                    d.flatten().any(|e| {
                        std::fs::read_to_string(e.path().join("type"))
                            .map(|t| t.trim().eq_ignore_ascii_case("battery"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        {
            assert!(ac || b > 0.0, "real battery detected; values must be sane");
        } else {
            assert_eq!((b, ac), (1.0, true), "no battery → full + AC default");
        }
    }
}
