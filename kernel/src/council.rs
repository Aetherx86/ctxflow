//! Council protocol as a sans-IO state machine. The host only performs the calls it is handed:
//! start -> independent answers -> (agreement stops here) -> anonymous peer scoring -> matrix
//! -> (clear winner stops here) -> one model resolves the disagreement.
use crate::compress::{compress, estimate, hash_id, redact, sanitize};
use crate::json::{obj, parse_loose, st, V};
use crate::prompt;
use alloc::{format, string::{String, ToString}, vec, vec::Vec};

const CARD_TOKENS: f64 = 420.0;
const REVIEW_TOKENS: f64 = 220.0;
const CHAIR_TOKENS: f64 = 480.0;
const MAX_SEATS: usize = 26;
const EPS: f64 = 1e-9;
const MAX_WEIGHT: f64 = 100.0;

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n).collect::<String>() + "..." }
}

/// Model text shown to another model or a person: no hidden characters, and on one line, so an
/// answer cannot forge a label line such as "B: ..." in a reviewer's prompt.
fn clean_line(s: &str, n: usize) -> String {
    let (t, _) = sanitize(s);
    clip(&t.split_whitespace().collect::<Vec<_>>().join(" "), n)
}

fn stance_norm(s: &str) -> String {
    let mut o = String::new();
    for w in s.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if !w.is_empty() {
            if !o.is_empty() {
                o.push(' ');
            }
            o.push_str(w);
        }
    }
    o
}

fn parse_card(text: &str) -> V {
    let list = |v: &V, n: usize| -> V {
        V::Arr(v.arr().iter().take(n).map(|x| st(&clean_line(x.s(), 240))).filter(|x| !x.s().is_empty()).collect())
    };
    if let Some(j) = parse_loose(text) {
        if !j.get("answer").s().is_empty() {
            return obj(vec![
                ("stance", st(&stance_norm(j.get("stance").s()))),
                ("answer", st(&clean_line(j.get("answer").s(), 600))),
                ("confidence", V::Num(j.get("confidence").n(50.0).clamp(0.0, 100.0))),
                ("reasons", list(j.get("reasons"), 3)),
                ("risks", list(j.get("risks"), 2)),
                ("would_change", st(&clean_line(j.get("would_change").s(), 240))),
                ("valid", V::Bool(true)),
            ]);
        }
    }
    // Local models often ignore the format. Keep the content; it just cannot count as agreement.
    obj(vec![("stance", st("")), ("answer", st(&clean_line(text, 600))), ("confidence", V::Num(50.0)), ("reasons", V::Arr(vec![])), ("risks", V::Arr(vec![])), ("would_change", st("")), ("valid", V::Bool(false))])
}

fn card_text(c: &V, label: &str) -> String {
    let join = |k: &str| c.get(k).arr().iter().map(|x| x.s()).collect::<Vec<_>>().join("; ");
    format!("{label}: {}\nconfidence {}\nreasons: {}\nrisks: {}\nwould change if: {}", c.get("answer").s(), c.get("confidence").n(50.0), join("reasons"), join("risks"), c.get("would_change").s())
}

fn card_md(c: &V, korean: bool) -> String {
    let t = |en: &'static str, ko: &'static str| if korean { ko } else { en };
    let mut o = format!("**{}**\n\n{}: {}%", c.get("answer").s(), t("Confidence", "확신도"), c.get("confidence").n(50.0));
    for (k, title) in [("reasons", t("Reasons", "근거")), ("risks", t("Risks", "위험"))] {
        let a = c.get(k).arr();
        if !a.is_empty() {
            o += &format!("\n\n{title}:");
            for x in a {
                o += &format!("\n- {}", x.s());
            }
        }
    }
    if !c.get("would_change").s().is_empty() {
        o += &format!("\n\n{}: {}", t("Would change if", "판단이 바뀌는 조건"), c.get("would_change").s());
    }
    o
}

/// Weak models ignore "the language of the question", so name it.
fn lang_line(q: &str) -> &'static str {
    let (mut hangul, mut kana, mut han) = (0, 0, 0);
    for c in q.chars() {
        match c as u32 {
            0xac00..=0xd7af | 0x1100..=0x11ff => hangul += 1,
            0x3040..=0x30ff => kana += 1,
            0x3400..=0x9fff => han += 1,
            _ => {}
        }
    }
    if hangul > 0 {
        "\nWrite every value in Korean."
    } else if kana > 0 {
        "\nWrite every value in Japanese."
    } else if han > 0 {
        "\nWrite every value in Chinese."
    } else {
        "\nWrite every value in English."
    }
}

fn ev(items: Vec<(&str, V)>) -> V {
    obj(items)
}

#[allow(clippy::too_many_arguments)]
fn call(id: &str, model: &str, fallbacks: &V, system: String, prompt: String, max: f64, seat: f64, kind: &str) -> V {
    obj(vec![("id", st(id)), ("model", st(model)), ("fallbacks", fallbacks.clone()), ("system", st(&system)), ("prompt", st(&prompt)), ("max_tokens", V::Num(max)), ("seat", V::Num(seat)), ("kind", st(kind))])
}

fn distinct_models(cards: &[V]) -> usize {
    let mut m: Vec<&str> = Vec::new();
    for c in cards {
        if !m.contains(&c.get("model").s()) {
            m.push(c.get("model").s());
        }
    }
    m.len()
}

/// Criterion weights for the weighted score. Missing, negative or non-numeric weights count as 1,
/// and all zero falls back to equal weights.
fn weights_from(limits: &V) -> (f64, f64, f64) {
    let w = limits.get("weights");
    let pick = |k: &str| {
        let v = w.get(k).n(1.0);
        // Capped, so a huge weight cannot overflow the weighted score to infinity.
        if v.is_finite() && v >= 0.0 { v.min(MAX_WEIGHT) } else { 1.0 }
    };
    let (e, r, f) = (pick("evidence"), pick("risk"), pick("feasibility"));
    if e + r + f > 0.0 { (e, r, f) } else { (1.0, 1.0, 1.0) }
}

fn weights_val(w: (f64, f64, f64)) -> V {
    obj(vec![("evidence", V::Num(w.0)), ("risk", V::Num(w.1)), ("feasibility", V::Num(w.2))])
}

/// Per-stage worst-case token breakdown: `(stage1, stage2, stage3)`.
pub fn worst_case_breakdown(q: &str, n: usize, mode: &str) -> (usize, usize, usize) {
    let qt = estimate(q);
    let sys = estimate(&prompt(0)) + 20;
    let n1 = n.saturating_sub(1);
    let s1 = n * (qt + sys + CARD_TOKENS as usize);
    if mode == "fast" {
        return (s1, 0, 0);
    }
    let s2 = n * (qt.min(700) + estimate(&prompt(1)) + n1 * CARD_TOKENS as usize + REVIEW_TOKENS as usize);
    let s3 = qt.min(700) + estimate(&prompt(2)) + 2 * CARD_TOKENS as usize + 200 + CHAIR_TOKENS as usize;
    (s1, s2, s3)
}

/// Worst case if every stage runs. Shown to the user before any call is made.
pub fn worst_case(q: &str, n: usize) -> usize {
    let (s1, s2, s3) = worst_case_breakdown(q, n, "auto");
    s1 + s2 + s3
}

/// Zero-token preflight analysis: per-stage token estimates, compression, sanitization, redaction,
/// and seat independence check.
pub fn preflight(i: &V) -> V {
    let raw_q = if !i.get("question").s().is_empty() { i.get("question").s() } else { i.get("text").s() };
    let (clean_q, hidden_removed) = sanitize(raw_q);
    let (_, rmap) = redact(&clean_q);
    let comp = compress(&clean_q, 12, 4);
    let mode = match i.get("mode").s() {
        "fast" => "fast",
        "thorough" => "thorough",
        _ => match i.get("limits").get("mode").s() {
            "fast" => "fast",
            "thorough" => "thorough",
            _ => "auto",
        },
    };
    let (seat_count, distinct) = match i.get("seats") {
        V::Arr(a) => {
            let mut uniq: Vec<&str> = Vec::new();
            for s in a {
                let m = match s {
                    V::Str(x) => x.as_str(),
                    V::Obj(_) => s.get("model").s(),
                    _ => "",
                };
                if !m.is_empty() && !uniq.contains(&m) {
                    uniq.push(m);
                }
            }
            (a.len(), uniq.len())
        }
        V::Num(n) if n.is_finite() && *n >= 0.0 => ((*n as usize).min(MAX_SEATS), (*n as usize).min(MAX_SEATS)),
        _ => (2, 2),
    };
    let effective = seat_count.clamp(2, MAX_SEATS);
    let (s1, s2, s3) = worst_case_breakdown(&clean_q, effective, mode);
    let weights = weights_from(i.get("limits"));
    obj(vec![
        ("mode", st(mode)),
        ("seats", V::Num(seat_count as f64)),
        ("distinct_models", V::Num(distinct as f64)),
        ("independent", V::Bool(distinct >= 2)),
        ("question_tokens", V::Num(estimate(&clean_q) as f64)),
        ("compressed_tokens", V::Num(estimate(&comp.text) as f64)),
        ("hidden_removed", V::Num(hidden_removed as f64)),
        ("redactions", V::Num(rmap.len() as f64)),
        ("stage1", V::Num(s1 as f64)),
        ("stage2", V::Num(s2 as f64)),
        ("stage3", V::Num(s3 as f64)),
        ("worst_case", V::Num((s1 + s2 + s3) as f64)),
        ("weights", weights_val(weights)),
    ])
}

fn finish(state: &mut V, events: &mut Vec<V>, mode: &str, final_text: &str, note: &str) -> V {
    let mut text = final_text.to_string();
    if !note.is_empty() {
        text += &format!("\n\n_{note}_");
    }
    state.set("stage", V::Num(9.0));
    let used = state.get("used").clone();
    let weights = weights_from(state.get("limits"));
    let result = obj(vec![
        ("mode", st(mode)),
        ("final", st(&text)),
        ("matrix", state.get("matrix").clone()),
        ("used", used.clone()),
        ("independent_models", V::Num(distinct_models(state.get("cards").arr()) as f64)),
        ("calls", state.get("calls").clone()),
        ("worst_case", state.get("worst_case").clone()),
        ("condorcet_winner", state.get("condorcet_winner").clone()),
        ("agreement", state.get("agreement").clone()),
        ("complete_reviewers", state.get("complete_reviewers").clone()),
        ("weights", weights_val(weights)),
    ]);
    events.push(ev(vec![("type", st("done")), ("decree", st(&text)), ("mode", st(mode)), ("used", used)]));
    result
}

pub fn step(i: &V) -> V {
    let mut events: Vec<V> = Vec::new();
    let mut calls: Vec<V> = Vec::new();
    let mut state;
    if let V::Obj(_) = i.get("start") {
        let s = i.get("start");
        let seats = s.get("seats").arr();
        let (clean_q, _) = sanitize(s.get("question").s());
        let question = clean_q.as_str();
        if seats.len() < 2 {
            return obj(vec![("error", st("a council needs at least 2 seats")), ("done", V::Bool(true))]);
        }
        if seats.len() > MAX_SEATS {
            return obj(vec![("error", st("a council supports at most 26 seats")), ("done", V::Bool(true))]);
        }
        let wc = worst_case(question, seats.len());
        let budget = s.get("limits").get("max_tokens").n(0.0);
        state = s.clone();
        state.set("question", st(question));
        state.set("worst_case", V::Num(wc as f64));
        state.set("question_short", st(&compress(question, 12, 4).text));
        state.set("used", obj(vec![("in", V::Num(0.0)), ("out", V::Num(0.0))]));
        state.set("cards", V::Arr(vec![]));
        state.set("matrix", V::Arr(vec![]));
        state.set("condorcet_winner", V::Null);
        state.set("agreement", V::Null);
        state.set("complete_reviewers", V::Num(0.0));
        state.set("calls", V::Num(seats.len() as f64));
        state.set("stage", V::Num(1.0));
        let opts: Vec<&str> = s.get("options").arr().iter().map(|x| x.s()).collect();
        let opt_line = if opts.is_empty() { String::new() } else { format!("\nstance must be exactly one of: {}", opts.join(" | ")) };
        events.push(ev(vec![("type", st("convened")), ("query", st(question)), ("models", V::Arr(seats.iter().map(|x| x.get("model").clone()).collect())), ("worst_case", V::Num(wc as f64)), ("budget", V::Num(budget))]));
        events.push(ev(vec![("type", st("stage")), ("stage", V::Num(1.0)), ("title", st("Independent answers"))]));
        for (k, seat) in seats.iter().enumerate() {
            let lens = seat.get("lens").n(k as f64) as usize % 4;
            let n = (k + 1) as f64;
            events.push(ev(vec![("type", st("seat_start")), ("seat", V::Num(n)), ("title", seat.get("title").clone()), ("model", seat.get("model").clone())]));
            calls.push(call(&format!("s{k}"), seat.get("model").s(), seat.get("fallbacks"), format!("{}{opt_line}{}\n{}", prompt(0), lang_line(question), prompt(4 + lens)), question.to_string(), CARD_TOKENS, n, "card"));
        }
        return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(false))]);
    }

    state = i.get("state").clone();
    let results = i.get("results").arr();
    let korean = lang_line(state.get("question").s()).contains("Korean");
    let (mut ui, mut uo) = (state.get("used").get("in").n(0.0), state.get("used").get("out").n(0.0));
    for r in results {
        ui += r.get("in").n(0.0);
        uo += r.get("out").n(0.0);
    }
    state.set("used", obj(vec![("in", V::Num(ui)), ("out", V::Num(uo))]));
    let budget = state.get("limits").get("max_tokens").n(0.0);
    let fits = |next: f64| budget <= 0.0 || ui + uo + next <= budget;
    let find = |id: &str| results.iter().find(|r| r.get("id").s() == id);
    let stage = state.get("stage").n(0.0) as i32;
    let seats: Vec<V> = state.get("seats").arr().to_vec();
    let qs = state.get("question_short").s().to_string();

    if stage == 1 {
        let mut cards: Vec<V> = Vec::new();
        for (k, seat) in seats.iter().enumerate() {
            let n = (k + 1) as f64;
            let r = find(&format!("s{k}"));
            match r.filter(|r| r.get("error") == &V::Null && !r.get("text").s().is_empty()) {
                Some(r) => {
                    let mut c = parse_card(r.get("text").s());
                    let model = if r.get("model").s().is_empty() { seat.get("model").s() } else { r.get("model").s() };
                    c.set("model", st(model));
                    c.set("seat", V::Num(n));
                    let md = card_md(&c, korean);
                    events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(n)), ("text", st(&md))]));
                    events.push(ev(vec![("type", st("seat_end")), ("seat", V::Num(n)), ("model", st(model)), ("opinion", st(&md))]));
                    cards.push(c);
                }
                None => {
                    let e = r.map_or("no result", |r| r.get("error").s());
                    events.push(ev(vec![("type", st("seat_error")), ("seat", V::Num(n)), ("error", st(&clip(e, 200)))]));
                    events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(n)), ("text", st(&format!("This model did not answer: {}", clip(e, 200))))]));
                }
            }
        }
        state.set("cards", V::Arr(cards.clone()));
        if cards.is_empty() {
            let res = finish(&mut state, &mut events, "failed", "No model answered, so there is no decision.", "");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let mut best = 0;
        for (k, c) in cards.iter().enumerate() {
            if c.get("confidence").n(0.0) > cards[best].get("confidence").n(0.0) {
                best = k;
            }
        }
        if cards.len() == 1 {
            let res = finish(&mut state, &mut events, "single", &card_md(&cards[0], korean), "Only one model answered, so nothing was cross-checked.");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let mode = state.get("limits").get("mode").s().to_string();
        if mode == "fast" {
            events.push(ev(vec![("type", st("stage")), ("stage", V::Num(2.0)), ("title", st("Scoring skipped")), ("model", st("none"))]));
            events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(5.0)), ("text", st("Skipped in Fast mode."))]));
            events.push(ev(vec![("type", st("stage")), ("stage", V::Num(3.0)), ("title", st("Decision")), ("model", st("none"))]));
            let res = finish(&mut state, &mut events, "fast", &card_md(&cards[best], korean), "Fast mode: this is the most confident answer and it was not cross-checked.");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let first = cards[0].get("stance").s();
        let min_conf = state.get("limits").get("agree_confidence").n(70.0);
        let agreed = cards.iter().all(|c| c.get("valid").b() && !first.is_empty() && c.get("stance").s() == first && c.get("confidence").n(0.0) >= min_conf);
        // Copies of one model agreeing is not independent evidence, so it does not stop the run.
        if mode != "thorough" && agreed && distinct_models(&cards) >= 2 && cards.len() == seats.len() {
            events.push(ev(vec![("type", st("stage")), ("stage", V::Num(2.0)), ("title", st("Scoring skipped")), ("model", st("none"))]));
            events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(5.0)), ("text", st(&format!("Skipped. All {} models independently chose \"{first}\" with confidence at or above {min_conf}, so scoring would cost tokens without changing the result.", cards.len())))]));
            events.push(ev(vec![("type", st("stage")), ("stage", V::Num(3.0)), ("title", st("Decision")), ("model", st("none"))]));
            let res = finish(&mut state, &mut events, "agreed", &card_md(&cards[best], korean), "");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let n1 = cards.len() - 1;
        let est = cards.len() as f64 * (((estimate(&qs) + estimate(&prompt(1))) + n1 * 160) as f64 + REVIEW_TOKENS);
        if !fits(est) {
            let res = finish(&mut state, &mut events, "budget", &card_md(&cards[best], korean), "Token limit reached before scoring. This is the most confident answer and it was not cross-checked.");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        events.push(ev(vec![("type", st("stage")), ("stage", V::Num(2.0)), ("title", st("Anonymous scoring")), ("model", st("all"))]));
        // Each reviewer sees the others under its own shuffled labels. Models and lenses stay hidden.
        let mut maps: Vec<V> = Vec::new();
        for (r, c) in cards.iter().enumerate() {
            let mut order: Vec<usize> = (0..cards.len()).filter(|x| *x != r).collect();
            order.sort_by_key(|x| hash_id(&format!("{r}:{x}:{qs}")));
            let mut body = String::new();
            let mut map: Vec<V> = Vec::new();
            let mut labels: Vec<String> = Vec::new();
            for (li, ci) in order.iter().enumerate() {
                let label = char::from(b'A' + li as u8).to_string();
                body += &format!("{}\n\n", card_text(&cards[*ci], &label));
                labels.push(label.clone());
                map.push(obj(vec![("label", st(&label)), ("card", V::Num(*ci as f64))]));
            }
            maps.push(V::Arr(map));
            calls.push(call(&format!("r{r}"), c.get("model").s(), &V::Arr(vec![]), format!("{}\nThe scores object must have one entry for each of: {}.", prompt(1), labels.join(", ")), format!("QUESTION:\n{qs}\n\nANSWERS:\n{body}"), REVIEW_TOKENS, c.get("seat").n(1.0), "review"));
        }
        state.set("maps", V::Arr(maps));
        state.set("calls", V::Num(state.get("calls").n(0.0) + cards.len() as f64));
        state.set("stage", V::Num(2.0));
        return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(false))]);
    }

    if stage == 2 {
        let cards = state.get("cards").arr().to_vec();
        let n = cards.len();
        if n == 0 {
            let res = finish(&mut state, &mut events, "failed", "No model answered, so there is no decision.", "");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let (we, wr, wf) = weights_from(state.get("limits"));
        let w_sum = we + wr + wf;
        let (mut ev_s, mut risk, mut feas, mut borda) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        let mut weighted_sum = vec![0.0f64; n];
        let mut min_w = vec![f64::INFINITY; n];
        let mut max_w = vec![f64::NEG_INFINITY; n];
        let mut votes = vec![0.0f64; n];
        let mut firsts = vec![0.0f64; n];
        let mut saw = vec![0.0f64; n];
        let mut pair_wins = vec![0u32; n * n];
        let mut complete = 0usize;
        let mut fatal: Vec<Vec<String>> = vec![Vec::new(); n];
        let safe_score = |v: &V| {
            let x = v.n(3.0);
            if x.is_finite() { x.clamp(1.0, 5.0) } else { 3.0 }
        };
        for r in 0..n {
            let Some(res) = find(&format!("r{r}")) else { continue };
            let Some(j) = parse_loose(res.get("text").s()) else { continue };
            let map = state.get("maps").arr().get(r).map_or(&[][..], |m| m.arr());
            let mut totals: Vec<(usize, f64)> = Vec::new();
            let mut seen_ci: Vec<usize> = Vec::new();
            for m in map {
                let raw_ci = m.get("card").n(-1.0);
                if !raw_ci.is_finite() || raw_ci < 0.0 {
                    continue;
                }
                let ci = raw_ci as usize;
                if ci >= n || seen_ci.contains(&ci) {
                    continue;
                }
                let s = j.get("scores").get(m.get("label").s());
                if !matches!(s, V::Obj(_)) {
                    continue;
                }
                seen_ci.push(ci);
                let ev_v = safe_score(s.get("evidence"));
                let ri_v = safe_score(s.get("risk"));
                let fe_v = safe_score(s.get("feasibility"));
                let w_score = (we * ev_v + wr * ri_v + wf * fe_v) / w_sum;
                ev_s[ci] += ev_v;
                risk[ci] += ri_v;
                feas[ci] += fe_v;
                weighted_sum[ci] += w_score;
                if w_score < min_w[ci] {
                    min_w[ci] = w_score;
                }
                if w_score > max_w[ci] {
                    max_w[ci] = w_score;
                }
                votes[ci] += 1.0;
                totals.push((ci, w_score));
                let f = j.get("fatal").get(m.get("label").s()).s();
                if !f.is_empty() {
                    fatal[ci].push(clean_line(f, 160));
                }
            }
            if totals.is_empty() {
                continue;
            }
            totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
            let m = totals.len();
            // Only a reviewer who scored every answer it was shown can name a winner, and a tie
            // at the top is not a win for anyone.
            if m >= 2 && m == map.len() {
                complete += 1;
                for (ci, _) in &totals {
                    saw[*ci] += 1.0;
                }
                if totals[0].1 > totals[1].1 + EPS {
                    firsts[totals[0].0] += 1.0;
                }
                for a in 0..m {
                    for b in (a + 1)..m {
                        if totals[a].1 > totals[b].1 + EPS {
                            pair_wins[totals[a].0 * n + totals[b].0] += 1;
                        }
                    }
                }
            }
            // Fractional Borda: tied answers split the Borda points for their shared ranks evenly.
            let mut idx = 0;
            while idx < m {
                let mut end = idx + 1;
                while end < m && (totals[idx].1 - totals[end].1).abs() <= EPS {
                    end += 1;
                }
                let avg_borda = ((m - 1 - idx) + (m - end)) as f64 / 2.0;
                for (ci, _) in &totals[idx..end] {
                    borda[*ci] += avg_borda;
                }
                idx = end;
            }
        }
        let mut pw_wins = vec![0usize; n];
        let mut condorcet: Option<usize> = None;
        let (mut agree_num, mut agree_den) = (0u64, 0u64);
        for a in 0..n {
            let mut beats_all = complete >= 1 && n >= 2;
            for b in 0..n {
                if a == b {
                    continue;
                }
                let ab = pair_wins[a * n + b];
                let ba = pair_wins[b * n + a];
                if ab > ba {
                    pw_wins[a] += 1;
                } else {
                    beats_all = false;
                }
                if a < b && ab + ba > 0 {
                    agree_num += ab.max(ba) as u64;
                    agree_den += (ab + ba) as u64;
                }
            }
            if beats_all {
                condorcet = Some(a);
            }
        }
        let agreement = if agree_den > 0 { Some(agree_num as f64 / agree_den as f64) } else { None };

        let mut rows: Vec<(usize, f64, f64)> = (0..n)
            .map(|k| {
                if votes[k] > 0.0 {
                    (k, borda[k] / votes[k], weighted_sum[k] / votes[k])
                } else {
                    (k, -1.0, -1.0)
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            let bd = b.1 - a.1;
            if bd.abs() > EPS {
                return bd.partial_cmp(&0.0).unwrap_or(core::cmp::Ordering::Equal);
            }
            let wd = b.2 - a.2;
            if wd.abs() > EPS {
                return wd.partial_cmp(&0.0).unwrap_or(core::cmp::Ordering::Equal);
            }
            fatal[a.0].len().cmp(&fatal[b.0].len()).then_with(|| {
                cards[b.0].get("confidence").n(0.0).partial_cmp(&cards[a.0].get("confidence").n(0.0)).unwrap_or(core::cmp::Ordering::Equal)
            })
        });
        let mut matrix: Vec<V> = Vec::new();
        let mut md = String::from("| Rank | Seat | Answer | Evidence | Risks | Feasible | Borda | Flaws |\n|---|---|---|---|---|---|---|---|\n");
        for (rank, (k, b, w_avg)) in rows.iter().enumerate() {
            let v = votes[*k].max(1.0);
            let c = &cards[*k];
            let seat = c.get("seat").n(0.0);
            let title = (seat as usize).checked_sub(1).and_then(|idx| seats.get(idx)).map_or("", |s| s.get("title").s());
            let scored = votes[*k] > 0.0;
            // An answer nobody scored has no score. Showing 0 would read as "worst".
            let cell = |x: f64, d: usize| if scored { format!("{:.*}", d, x) } else { String::from("-") };
            md += &format!("| {} | {} | {} | {} | {} | {} | {} | {} |\n", rank + 1, title, clip(c.get("answer").s(), 90).replace('|', "/").replace('\n', " "), cell(ev_s[*k] / v, 1), cell(risk[*k] / v, 1), cell(feas[*k] / v, 1), cell(*b, 2), fatal[*k].len());
            let num = |x: f64| if scored { V::Num(x) } else { V::Null };
            let spread = if scored { V::Num((max_w[*k] - min_w[*k]).max(0.0)) } else { V::Null };
            let first_share = if saw[*k] > 0.0 { V::Num(firsts[*k] / saw[*k]) } else { V::Null };
            matrix.push(obj(vec![
                ("seat", V::Num(seat)),
                ("model", c.get("model").clone()),
                ("answer", c.get("answer").clone()),
                ("evidence", num(ev_s[*k] / v)),
                ("risk", num(risk[*k] / v)),
                ("feasibility", num(feas[*k] / v)),
                ("weighted", num(*w_avg)),
                ("spread", spread),
                ("borda", num(*b)),
                ("votes", V::Num(votes[*k])),
                ("first_share", first_share),
                ("pairwise_wins", V::Num(pw_wins[*k] as f64)),
                ("fatal", V::Arr(fatal[*k].iter().map(|f| st(f)).collect())),
            ]));
        }
        let top = rows[0].0;
        // Share of the reviewers who saw the top answer and ranked it strictly first.
        let share = if saw[top] > 0.0 { firsts[top] / saw[top] } else { 0.0 };
        state.set("matrix", V::Arr(matrix));
        state.set("share", V::Num(share));
        state.set("complete_reviewers", V::Num(complete as f64));
        state.set("agreement", agreement.map_or(V::Null, V::Num));
        state.set("condorcet_winner", match condorcet {
            Some(ci) => obj(vec![("seat", cards[ci].get("seat").clone()), ("model", cards[ci].get("model").clone())]),
            None => V::Null,
        });
        events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(5.0)), ("text", st(&md))]));
        events.push(ev(vec![("type", st("stage_end")), ("stage", V::Num(2.0)), ("content", st(&md))]));
        let chair = state.get("chair").get("model").s().to_string();
        let need = state.get("limits").get("clear_share").n(0.75);
        events.push(ev(vec![("type", st("stage")), ("stage", V::Num(3.0)), ("title", st("Decision")), ("seat", V::Num(5.0)), ("model", st(&chair))]));
        if state.get("limits").get("mode").s() != "thorough" && complete >= 2 && share >= need && fatal[top].is_empty() {
            let res = finish(&mut state, &mut events, "ranked", &card_md(&cards[top], korean), "");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        let mut material = format!("QUESTION:\n{qs}\n\n");
        for (rank, (k, _, _)) in rows.iter().take(2).enumerate() {
            material += &format!("{}\n\n", card_text(&cards[*k], if rank == 0 { "Top answer" } else { "Runner-up" }));
        }
        material += &format!("SCORES:\n{md}");
        let flaws: Vec<String> = rows.iter().take(2).flat_map(|(k, _, _)| fatal[*k].iter().cloned()).collect();
        if !flaws.is_empty() {
            material += &format!("\nREPORTED FLAWS:\n- {}\n", flaws.join("\n- "));
        }
        let est = (estimate(&material) + estimate(&prompt(2))) as f64 + CHAIR_TOKENS;
        if !fits(est) || chair.is_empty() {
            let res = finish(&mut state, &mut events, "budget", &card_md(&cards[top], korean), "Token limit reached before the final step. This is the top-scored answer and the disagreement was not resolved.");
            return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)]);
        }
        state.set("top", V::Num(top as f64));
        calls.push(call("chair", &chair, state.get("chair").get("fallbacks"), format!("{}{}", prompt(2), lang_line(&qs)), material, CHAIR_TOKENS, 5.0, "chair"));
        state.set("calls", V::Num(state.get("calls").n(0.0) + 1.0));
        state.set("stage", V::Num(3.0));
        return obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(false))]);
    }

    // stage 3
    let top = state.get("top").n(0.0) as usize;
    let top_card = state.get("cards").arr().get(top).map(|c| card_md(c, korean)).unwrap_or_default();
    let res = match find("chair").filter(|r| r.get("error") == &V::Null && !r.get("text").s().is_empty()) {
        Some(r) => {
            let (t, _) = sanitize(r.get("text").s());
            events.push(ev(vec![("type", st("chunk")), ("seat", V::Num(5.0)), ("text", st(&t))]));
            finish(&mut state, &mut events, "resolved", &t, "")
        }
        None => finish(&mut state, &mut events, "chair_failed", &top_card, "The final model failed. This is the top-scored answer and the disagreement was not resolved."),
    };
    obj(vec![("state", state), ("calls", V::Arr(calls)), ("events", V::Arr(events)), ("done", V::Bool(true)), ("result", res)])
}
