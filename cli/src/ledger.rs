//! Usage ledger and budget guard. Global (~/.ctxflow/usage.jsonl), as free-tier limits are per
//! account rather than per repository.
use crate::backends::is_local;
use crate::config::model_budget;
use crate::util::{civil_from_days, ctxflow_home, days_from_civil, now};
use ctxflow_kernel::json::{obj, parse, st, V};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn ledger_path() -> PathBuf {
    let home = ctxflow_home();
    let _ = fs::create_dir_all(&home);
    home.join("usage.jsonl")
}

#[allow(clippy::too_many_arguments)]
pub fn record(spec: &str, input: u64, output: u64, kind: &str, estimated: bool, project: &str, cost_usd: Option<f64>) {
    let mut items =
        vec![("ts", V::Num(now())), ("model", st(spec)), ("in", V::Num(input as f64)), ("out", V::Num(output as f64)), ("kind", st(kind)), ("est", V::Bool(estimated)), ("project", st(project))];
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
    if t >= starts && t < ends {
        -7 * 3600
    } else {
        -8 * 3600
    }
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
    entries_since(pacific_midnight(now()))
        .iter()
        .filter(|e| {
            let backend = e.get("model").s().split(':').next().unwrap_or("");
            backend != "echo" && !is_local(backend, cfg)
        })
        .map(|e| e.get("in").n(0.0) as u64 + e.get("out").n(0.0) as u64)
        .sum()
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

#[derive(Clone, Default, Debug)]
pub struct ModelStats {
    pub calls: u64,
    pub input: u64,
    pub output: u64,
    pub local: bool,
    pub recorded_cost_usd: f64,
    pub estimated_cost_usd: f64,
}

#[derive(Clone, Default, Debug)]
pub struct KindStats {
    pub calls: u64,
    pub input: u64,
    pub output: u64,
    pub cost_usd: f64,
}

#[derive(Clone, Default, Debug)]
pub struct DayStats {
    pub calls: u64,
    pub input: u64,
    pub output: u64,
    pub cloud_tokens: u64,
    pub local_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Clone, Default, Debug)]
pub struct StatsReport {
    pub days: Option<u64>,
    pub project: Option<String>,
    pub entries: u64,
    pub skipped_lines: u64,
    pub total_in: u64,
    pub total_out: u64,
    pub cloud_calls: u64,
    pub local_calls: u64,
    pub cloud_tokens: u64,
    pub local_tokens: u64,
    pub recorded_cost_usd: f64,
    pub estimated_cost_usd: f64,
    pub by_model: BTreeMap<String, ModelStats>,
    pub by_kind: BTreeMap<String, KindStats>,
    pub by_day: BTreeMap<String, DayStats>,
}

impl StatsReport {
    pub fn total_tokens(&self) -> u64 {
        self.total_in.saturating_add(self.total_out)
    }

    pub fn total_cost_usd(&self) -> f64 {
        self.recorded_cost_usd + self.estimated_cost_usd
    }

    pub fn to_v(&self) -> V {
        let models: Vec<V> = self
            .by_model
            .iter()
            .map(|(model, s)| {
                obj(vec![
                    ("model", st(model)),
                    ("calls", V::Num(s.calls as f64)),
                    ("in", V::Num(s.input as f64)),
                    ("out", V::Num(s.output as f64)),
                    ("total", V::Num((s.input + s.output) as f64)),
                    ("local", V::Bool(s.local)),
                    ("recorded_cost_usd", V::Num(s.recorded_cost_usd)),
                    ("estimated_cost_usd", V::Num(s.estimated_cost_usd)),
                    ("cost_usd", V::Num(s.recorded_cost_usd + s.estimated_cost_usd)),
                ])
            })
            .collect();
        let kinds: Vec<V> = self
            .by_kind
            .iter()
            .map(|(kind, s)| {
                obj(vec![
                    ("kind", st(kind)),
                    ("calls", V::Num(s.calls as f64)),
                    ("in", V::Num(s.input as f64)),
                    ("out", V::Num(s.output as f64)),
                    ("total", V::Num((s.input + s.output) as f64)),
                    ("cost_usd", V::Num(s.cost_usd)),
                ])
            })
            .collect();
        let daily: Vec<V> = self
            .by_day
            .iter()
            .map(|(day, s)| {
                obj(vec![
                    ("date", st(day)),
                    ("calls", V::Num(s.calls as f64)),
                    ("in", V::Num(s.input as f64)),
                    ("out", V::Num(s.output as f64)),
                    ("cloud_tokens", V::Num(s.cloud_tokens as f64)),
                    ("local_tokens", V::Num(s.local_tokens as f64)),
                    ("cost_usd", V::Num(s.cost_usd)),
                ])
            })
            .collect();
        obj(vec![
            ("days", self.days.map_or(V::Null, |d| V::Num(d as f64))),
            ("project", self.project.as_deref().map_or(V::Null, st)),
            ("entries", V::Num(self.entries as f64)),
            ("skipped_lines", V::Num(self.skipped_lines as f64)),
            ("in", V::Num(self.total_in as f64)),
            ("out", V::Num(self.total_out as f64)),
            ("total_tokens", V::Num(self.total_tokens() as f64)),
            ("cloud_calls", V::Num(self.cloud_calls as f64)),
            ("local_calls", V::Num(self.local_calls as f64)),
            ("cloud_tokens", V::Num(self.cloud_tokens as f64)),
            ("local_tokens", V::Num(self.local_tokens as f64)),
            ("recorded_cost_usd", V::Num(self.recorded_cost_usd)),
            ("estimated_cost_usd", V::Num(self.estimated_cost_usd)),
            ("total_cost_usd", V::Num(self.total_cost_usd())),
            ("by_model", V::Arr(models)),
            ("by_kind", V::Arr(kinds)),
            ("by_day", V::Arr(daily)),
        ])
    }
}

fn nonneg_u64(v: &V) -> u64 {
    let n = v.n(0.0);
    if n.is_finite() && n > 0.0 {
        n as u64
    } else {
        0
    }
}

fn nonneg_f64(v: &V) -> Option<f64> {
    if let V::Num(n) = v {
        if n.is_finite() && *n >= 0.0 {
            return Some(*n);
        }
    }
    None
}

fn pricing_cost(cfg: &V, spec: &str, input: u64, output: u64) -> f64 {
    let p = cfg.get("pricing").get(spec);
    if !matches!(p, V::Obj(_)) {
        return 0.0;
    }
    let in_rate = nonneg_f64(p.get("input_per_mtok")).unwrap_or(0.0);
    let out_rate = nonneg_f64(p.get("output_per_mtok")).unwrap_or(0.0);
    (input as f64 * in_rate + output as f64 * out_rate) / 1_000_000.0
}

fn utc_day_string(ts: f64) -> String {
    let z = (ts as i64).div_euclid(86400);
    let (y, m, d) = civil_from_days(z);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Aggregates JSONL ledger text. Malformed lines and negative/non-finite fields are skipped safely.
pub fn summarize_text(text: &str, cfg: &V, now_ts: f64, days: Option<u64>, project_filter: Option<&str>) -> StatsReport {
    let cutoff = days.map(|d| now_ts - (d as f64) * 86400.0);
    let mut rep = StatsReport { days, project: project_filter.map(str::to_string), ..StatsReport::default() };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some(e) = parse(line) else {
            rep.skipped_lines += 1;
            continue;
        };
        if !matches!(e, V::Obj(_)) {
            rep.skipped_lines += 1;
            continue;
        }
        let ts = e.get("ts").n(0.0);
        if !ts.is_finite() || ts < 0.0 {
            rep.skipped_lines += 1;
            continue;
        }
        if let Some(min_ts) = cutoff {
            if ts < min_ts {
                continue;
            }
        }
        if let Some(proj) = project_filter {
            if e.get("project").s() != proj {
                continue;
            }
        }
        let model = e.get("model").s();
        if model.is_empty() {
            rep.skipped_lines += 1;
            continue;
        }
        let input = nonneg_u64(e.get("in"));
        let output = nonneg_u64(e.get("out"));
        let total = input.saturating_add(output);
        let kind = match e.get("kind").s() {
            "" => "other",
            k => k,
        };
        let backend = model.split(':').next().unwrap_or("");
        let local = backend == "echo" || is_local(backend, cfg);
        let (rec_cost, est_cost) = match nonneg_f64(e.get("cost_usd")) {
            Some(c) => (c, 0.0),
            None => (0.0, if local { 0.0 } else { pricing_cost(cfg, model, input, output) }),
        };
        let entry_cost = rec_cost + est_cost;

        rep.entries += 1;
        rep.total_in = rep.total_in.saturating_add(input);
        rep.total_out = rep.total_out.saturating_add(output);
        rep.recorded_cost_usd += rec_cost;
        rep.estimated_cost_usd += est_cost;
        if local {
            rep.local_calls += 1;
            rep.local_tokens = rep.local_tokens.saturating_add(total);
        } else {
            rep.cloud_calls += 1;
            rep.cloud_tokens = rep.cloud_tokens.saturating_add(total);
        }

        let mb = rep.by_model.entry(model.to_string()).or_default();
        mb.calls += 1;
        mb.input = mb.input.saturating_add(input);
        mb.output = mb.output.saturating_add(output);
        mb.local = local;
        mb.recorded_cost_usd += rec_cost;
        mb.estimated_cost_usd += est_cost;

        let kb = rep.by_kind.entry(kind.to_string()).or_default();
        kb.calls += 1;
        kb.input = kb.input.saturating_add(input);
        kb.output = kb.output.saturating_add(output);
        kb.cost_usd += entry_cost;

        let db = rep.by_day.entry(utc_day_string(ts)).or_default();
        db.calls += 1;
        db.input = db.input.saturating_add(input);
        db.output = db.output.saturating_add(output);
        if local {
            db.local_tokens = db.local_tokens.saturating_add(total);
        } else {
            db.cloud_tokens = db.cloud_tokens.saturating_add(total);
        }
        db.cost_usd += entry_cost;
    }
    rep
}

pub fn summarize(cfg: &V, days: Option<u64>, project_filter: Option<&str>) -> StatsReport {
    let text = fs::read_to_string(ledger_path()).unwrap_or_default();
    summarize_text(&text, cfg, now(), days, project_filter)
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

    #[test]
    fn summarize_aggregates_models_kinds_pricing_and_skips_garbage() {
        let now_ts = utc(2026, 11, 21, 12) as f64;
        let old_ts = utc(2026, 11, 1, 12) as f64;
        let cfg = obj(vec![("pricing", obj(vec![("gemini:flash", obj(vec![("input_per_mtok", V::Num(1.0)), ("output_per_mtok", V::Num(4.0))]))]))]);
        let lines = format!(
            "{{\"ts\":{now_ts},\"model\":\"gemini:flash\",\"in\":1000000,\"out\":500000,\"kind\":\"ask/light\",\"project\":\"/repoA\"}}\n\
             {{\"ts\":{now_ts},\"model\":\"claude:sonnet\",\"in\":2000,\"out\":1000,\"kind\":\"ask/heavy\",\"project\":\"/repoA\",\"cost_usd\":0.025}}\n\
             {{\"ts\":{now_ts},\"model\":\"ollama:qwen\",\"in\":5000,\"out\":500,\"kind\":\"council/card\",\"project\":\"/repoB\"}}\n\
             {{\"ts\":{old_ts},\"model\":\"gemini:flash\",\"in\":999,\"out\":1,\"kind\":\"ask/light\",\"project\":\"/repoA\"}}\n\
             not json at all\n\
             {{\"ts\":-5,\"model\":\"gemini:flash\",\"in\":10,\"out\":10}}\n"
        );
        let rep = summarize_text(&lines, &cfg, now_ts, Some(7), None);
        assert_eq!(rep.entries, 3);
        assert_eq!(rep.skipped_lines, 2);
        assert_eq!(rep.cloud_calls, 2);
        assert_eq!(rep.local_calls, 1);
        assert_eq!(rep.cloud_tokens, 1_503_000);
        assert_eq!(rep.local_tokens, 5_500);
        assert!((rep.recorded_cost_usd - 0.025).abs() < 1e-9);
        // 1M * $1 + 0.5M * $4 = $3.00
        assert!((rep.estimated_cost_usd - 3.0).abs() < 1e-9);
        assert_eq!(rep.by_day.len(), 1);
        assert!(rep.by_day.contains_key("2026-11-21"));

        let proj_only = summarize_text(&lines, &cfg, now_ts, None, Some("/repoA"));
        assert_eq!(proj_only.entries, 3);
        assert_eq!(proj_only.local_calls, 0);
    }
}
