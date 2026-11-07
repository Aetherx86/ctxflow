//! Usage ledger and budget guard. Global (~/.ctxflow/usage.jsonl), as free-tier limits are per
//! account rather than per repository.
use crate::backends::is_local;
use crate::config::model_budget;
use crate::util::{civil_from_days, days_from_civil, home_dir, now};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn ledger_path() -> PathBuf {
    let home = std::env::var_os("CTXFLOW_HOME").map(PathBuf::from).unwrap_or_else(|| home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".ctxflow"));
    let _ = fs::create_dir_all(&home);
    home.join("usage.jsonl")
}

#[allow(clippy::too_many_arguments)]
pub fn record(spec: &str, input: u64, output: u64, kind: &str, estimated: bool, project: &str, cost_usd: Option<f64>) {
    let mut items = vec![
        ("ts", V::Num(now())),
        ("model", st(spec)),
        ("in", V::Num(input as f64)),
        ("out", V::Num(output as f64)),
        ("kind", st(kind)),
        ("est", V::Bool(estimated)),
        ("project", st(project)),
    ];
    if let Some(c) = cost_usd {
        items.push(("cost_usd", V::Num(c)));
    }
    // One write per line, so parallel calls do not interleave.
    let line = format!("{}\n", obj(items).dump());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(ledger_path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn nth_sunday(year: i64, month: i64, n: i64) -> i64 {
    let first = days_from_civil(year, month, 1);
    let weekday = (first + 4).rem_euclid(7); // 0 = Sunday; 1970-01-01 was a Thursday
    first + (7 - weekday) % 7 + 7 * (n - 1)
}

/// Pacific offset in seconds at a UTC instant, by the current US rule.
fn pacific_offset(t: i64) -> i64 {
    let (year, _, _) = civil_from_days((t - 8 * 3600).div_euclid(86400));
    let starts = nth_sunday(year, 3, 2) * 86400 + 10 * 3600; // 02:00 PST
    let ends = nth_sunday(year, 11, 1) * 86400 + 9 * 3600; // 02:00 PDT
    if t >= starts && t < ends { -7 * 3600 } else { -8 * 3600 }
}

/// Gemini daily quotas reset at midnight Pacific.
pub fn pacific_midnight(t: f64) -> f64 {
    let t = t as i64;
    let local_day = (t + pacific_offset(t)).div_euclid(86400);
    let guess = local_day * 86400 - pacific_offset(t);
    (local_day * 86400 - pacific_offset(guess)) as f64
}

fn entries_since(since: f64) -> Vec<V> {
    let text = fs::read_to_string(ledger_path()).unwrap_or_default();
    text.lines().filter_map(parse).filter(|e| e.get("ts").n(0.0) >= since).collect()
}

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub rpm: u64,
    pub tpm: u64,
    pub rpd: u64,
    pub tpd: u64,
}

impl Usage {
    pub fn field(&self, key: &str) -> u64 {
        match key {
            "rpm" => self.rpm,
            "tpm" => self.tpm,
            "rpd" => self.rpd,
            _ => self.tpd,
        }
    }
}

/// Per-model counters: requests and tokens in the last minute and since the daily reset.
pub fn usage_window() -> BTreeMap<String, Usage> {
    let t = now();
    let day_start = pacific_midnight(t);
    let mut stats: BTreeMap<String, Usage> = BTreeMap::new();
    for e in entries_since(day_start.min(t - 60.0)) {
        let u = stats.entry(e.get("model").s().to_string()).or_default();
        let (input, output) = (e.get("in").n(0.0) as u64, e.get("out").n(0.0) as u64);
        let ts = e.get("ts").n(0.0);
        if ts >= t - 60.0 {
            u.rpm += 1;
            u.tpm += input;
        }
        if ts >= day_start {
            u.rpd += 1;
            u.tpd += input + output;
        }
    }
    stats
}

/// All tokens spent since the daily reset, across models. Local models cost nothing.
pub fn total_today(cfg: &V) -> u64 {
    entries_since(pacific_midnight(now())).iter().filter(|e| {
        let backend = e.get("model").s().split(':').next().unwrap_or("");
        backend != "echo" && !is_local(backend, cfg)
    }).map(|e| e.get("in").n(0.0) as u64 + e.get("out").n(0.0) as u64).sum()
}

/// (reason, minute_only). No reason means the call fits. minute_only means waiting is an option.
pub fn check(cfg: &V, spec: &str, est_input: u64, est_output: u64) -> (Option<String>, bool) {
    let limits = model_budget(cfg, spec);
    if !matches!(limits, V::Obj(_)) {
        return (None, false);
    }
    let ratio = cfg.get("budget").get("safety_ratio").n(0.9);
    let used = usage_window().get(spec).copied().unwrap_or_default();
    let mut hits: Vec<(&str, String)> = Vec::new();
    for (key, planned) in [("rpd", 1), ("tpd", est_input + est_output), ("rpm", 1), ("tpm", est_input)] {
        let limit = limits.get(key).n(0.0) as u64;
        if limit > 0 && used.field(key) + planned > ((limit as f64 * ratio) as u64).max(1) {
            hits.push((key, format!("{key} {}+{planned} > {:.0}% of {limit}", used.field(key), ratio * 100.0)));
        }
    }
    if hits.is_empty() {
        return (None, false);
    }
    let minute_only = hits.iter().all(|(k, _)| *k == "rpm" || *k == "tpm");
    (Some(hits.into_iter().map(|(_, r)| r).collect::<Vec<_>>().join("; ")), minute_only)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i64, m: i64, d: i64, h: i64) -> i64 {
        days_from_civil(y, m, d) * 86400 + h * 3600
    }

    #[test]
    fn dates_round_trip() {
        for z in [-719468, -1, 0, 1, 18000, 20767, 40000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn pacific_offset_follows_daylight_time() {
        assert_eq!(pacific_offset(utc(2026, 1, 15, 12)), -8 * 3600);
        assert_eq!(pacific_offset(utc(2026, 7, 15, 12)), -7 * 3600);
        // 2026: starts Sunday March 8, ends Sunday November 1.
        assert_eq!(pacific_offset(utc(2026, 3, 8, 9)), -8 * 3600);
        assert_eq!(pacific_offset(utc(2026, 3, 8, 10)), -7 * 3600);
        assert_eq!(pacific_offset(utc(2026, 11, 1, 8)), -7 * 3600);
        assert_eq!(pacific_offset(utc(2026, 11, 1, 9)), -8 * 3600);
    }

    #[test]
    fn pacific_midnight_finds_the_local_day_start() {
        // 2026-11-08 18:00 UTC is 10:00 PST, so midnight was 08:00 UTC.
        assert_eq!(pacific_midnight(utc(2026, 11, 8, 18) as f64), utc(2026, 11, 8, 8) as f64);
        // 2026-07-15 03:00 UTC is still July 14 in Pacific; midnight was July 14 07:00 UTC.
        assert_eq!(pacific_midnight(utc(2026, 7, 15, 3) as f64), utc(2026, 7, 14, 7) as f64);
        // On the change day, midnight is under the offset before the change.
        assert_eq!(pacific_midnight(utc(2026, 3, 8, 20) as f64), utc(2026, 3, 8, 8) as f64);
        assert_eq!(pacific_midnight(utc(2026, 11, 1, 20) as f64), utc(2026, 11, 1, 7) as f64);
    }
}
