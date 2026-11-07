//! Runs the council protocol. The kernel decides everything; this file only performs the calls.
use crate::backends;
use crate::config;
use crate::engine::Engine;
use crate::ledger;
use crate::util::{k, log, str_arr, R};
use ctxflow_kernel::compress::estimate;
use ctxflow_kernel::json::{obj, st, V};

pub struct Preflight {
    pub worst_case: u64,
    pub daily: u64,
    pub left_today: u64,
}

/// Worst-case cost and how much of today's allowance remains. Makes no calls.
pub fn preflight(eng: &Engine, question: &str, seats: usize) -> Preflight {
    let seat = obj(vec![("model", st("x")), ("title", st("")), ("fallbacks", V::Arr(vec![]))]);
    let probe = k("council_step", vec![("start", obj(vec![
        ("question", st(question)),
        ("seats", V::Arr(vec![seat; seats.max(2)])),
        ("chair", obj(vec![("model", st("x")), ("fallbacks", V::Arr(vec![]))])),
        ("options", V::Arr(vec![])),
        ("limits", V::Obj(vec![])),
    ]))]);
    let daily = eng.cfg.get("council").get("daily_tokens").n(0.0) as u64;
    Preflight {
        worst_case: probe.get("state").get("worst_case").n(0.0) as u64,
        daily,
        left_today: if daily > 0 { daily.saturating_sub(ledger::total_today(&eng.cfg)) } else { 0 },
    }
}

fn execute(eng: &Engine, call: &V) -> V {
    let mut errors = Vec::new();
    let id = call.get("id").s();
    let (system, prompt) = (call.get("system").s(), call.get("prompt").s());
    let max = call.get("max_tokens").n(500.0) as u64;
    let chain = std::iter::once(call.get("model").s().to_string()).chain(call.get("fallbacks").arr().iter().map(|f| f.s().to_string()));
    for spec in chain {
        if let (Some(reason), _) = ledger::check(&eng.cfg, &spec, estimate(prompt) as u64, max) {
            errors.push(format!("{spec}: {reason}"));
            continue;
        }
        match backends::call(&spec, system, prompt, &eng.cfg, Some(max)) {
            Ok(r) => {
                let input = if r.estimated && r.input_tokens == 0 { estimate(prompt) as u64 } else { r.input_tokens };
                let output = if r.estimated && r.output_tokens == 0 { estimate(&r.text) as u64 } else { r.output_tokens };
                ledger::record(&spec, input, output, &format!("council/{}", call.get("kind").s()), r.estimated, &eng.p.root.display().to_string(), r.cost_usd);
                return obj(vec![
                    ("id", st(id)),
                    ("text", st(&r.text)),
                    ("model", st(&spec)),
                    ("in", V::Num(input as f64)),
                    ("out", V::Num(output as f64)),
                    ("redacted", V::Num(r.redacted as f64)),
                ]);
            }
            Err(e) => {
                log(&format!("{spec} failed: {}", e.msg));
                errors.push(format!("{spec}: {}", e.msg));
            }
        }
    }
    let joined = if errors.is_empty() { "no model".to_string() } else { errors.join("; ") };
    obj(vec![("id", st(id)), ("error", st(&joined))])
}

pub fn run(eng: &Engine, question: &str, options: &[String], on_event: &mut dyn FnMut(&V)) -> R<V> {
    let (seats, chair, fallbacks) = config::council_seats(&eng.cfg);
    let c = eng.cfg.get("council");
    let mut limit = c.get("max_tokens").n(20000.0) as u64;
    let pre = preflight(eng, question, seats.len());
    if pre.daily > 0 {
        if pre.left_today == 0 {
            return Err(format!("today's limit of {} tokens is used up. Use a local profile or raise [council] daily_tokens.", pre.daily));
        }
        limit = limit.min(pre.left_today);
    }
    let seat_list: Vec<V> = seats.iter().enumerate().map(|(i, m)| obj(vec![
        ("model", st(m)),
        ("title", st(m)),
        ("lens", V::Num(i as f64)),
        ("fallbacks", str_arr(&fallbacks)),
    ])).collect();
    let mut step = k("council_step", vec![("start", obj(vec![
        ("question", st(question)),
        ("options", str_arr(options)),
        ("seats", V::Arr(seat_list)),
        ("chair", obj(vec![("model", st(&chair)), ("fallbacks", str_arr(&fallbacks))])),
        ("limits", obj(vec![
            ("mode", eng.cfg.get("mode").clone()),
            ("max_tokens", V::Num(limit as f64)),
            ("agree_confidence", c.get("agree_confidence").clone()),
            ("clear_share", c.get("clear_share").clone()),
        ])),
    ]))]);
    if !step.get("error").s().is_empty() {
        return Err(format!("{}. This profile has too few seats.", step.get("error").s()));
    }
    let mut redacted = 0.0;
    loop {
        for ev in step.get("events").arr() {
            on_event(ev);
        }
        if step.get("done").b() {
            let mut result = step.get("result").clone();
            result.set("redacted", V::Num(redacted));
            return Ok(result);
        }
        // Calls in a stage are independent, so they run at the same time. The stage takes as long
        // as its slowest model, not the sum of all of them.
        let calls = step.get("calls").arr();
        let results: Vec<V> = std::thread::scope(|scope| {
            let handles: Vec<_> = calls.iter().map(|c| scope.spawn(move || execute(eng, c))).collect();
            handles.into_iter().map(|h| h.join().unwrap_or(V::Null)).collect()
        });
        redacted += results.iter().map(|r| r.get("redacted").n(0.0)).sum::<f64>();
        step = k("council_step", vec![("state", step.get("state").clone()), ("results", V::Arr(results))]);
    }
}
