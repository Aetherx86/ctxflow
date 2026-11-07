use super::*;
use crate::compress::*;
use crate::council::worst_case;
use crate::json::parse;

fn run(j: &str) -> V { parse(&dispatch(j)).unwrap() }

#[test]
fn json_roundtrip_unicode_and_depth() {
    let v = parse(r#"{"a":"한글\n😀","b":[1,2.5,true,null]}"#).unwrap();
    assert_eq!(v.get("a").s(), "한글\n😀");
    assert_eq!(parse(&v.dump()).unwrap(), v);
    assert_eq!(parse("1e999").unwrap().dump(), "null");
    assert_eq!(V::Num(0.0002459).dump(), "0.0002459");
    assert!(parse(&"[".repeat(200)).is_none());
    assert!(parse("{\"a\":").is_none());
}

#[test]
fn compress_is_reversible_and_smaller() {
    let code: String = (0..50).map(|i| format!("    x{i} = {i}\n")).collect();
    let block = format!("```py\ndef login(a):\n{code}```");
    let c = compress(&format!("fix this\n{block}\nthanks"), 12, 6);
    assert_eq!(c.stubs.len(), 1);
    assert_eq!(c.stubs[0].get("body").s(), block, "original must come back byte for byte");
    assert!(c.text.contains("defs: login"));
    assert!(c.text.len() < block.len() / 5);
    assert!(c.text.contains("thanks"));
}

#[test]
fn short_code_and_prose_stay_verbatim() {
    let t = "see `a.py`\n```js\nconst a = 1;\n```\nend";
    let c = compress(t, 12, 6);
    assert_eq!(c.text, t);
    assert!(c.stubs.is_empty());
}

#[test]
fn logs_collapse_repeats_and_keep_errors() {
    let mut t = String::from("run\n");
    for i in 0..200 { t += &format!("2026-11-07 10:00:{i:02} INFO tick {i}\n"); }
    t += "2026-11-07 10:03:20 ERROR db timeout on host-3\nValueError: boom";
    let c = compress(&t, 12, 6);
    assert!(c.text.contains("db timeout"));
    assert!(c.text.contains("ValueError: boom"));
    assert!(c.text.contains("(x"));
    assert_eq!(c.stubs.len(), 1);
    assert!(estimate(&c.text) * 20 < estimate(&t));
}

#[test]
fn duplicate_block_becomes_reference() {
    let code: String = (0..30).map(|i| format!("l{i}\n")).collect();
    let b = format!("```\n{code}```");
    let c = compress(&format!("{b}\nagain\n{b}"), 12, 6);
    assert_eq!(c.stubs.len(), 1);
    assert!(c.text.contains("[same as #"));
}

#[test]
fn anchors_catch_what_summaries_drop() {
    let t = "Fix refresh_token() in src/auth/session.ts at v1.82.6, commit a1b2c3d\nTypeError: x is not a function\n[#0123456789ab code 40 lines]\ncost 579k and $2.31";
    let a = anchors(t, 40);
    for want in ["refresh_token", "src/auth/session.ts", "1.82.6", "a1b2c3d", "TypeError: x is not a function", "#0123456789ab", "579k", "$2.31"] {
        assert!(a.iter().any(|x| x.contains(want)), "missing {want}: {a:?}");
    }
    assert!(!a.iter().any(|x| x == "if"));
}

#[test]
fn verify_summary_restores_missing_refs() {
    let pins = vec![st("src/a.py"), st("TypeError")];
    let (s, m) = verify_summary("- fixed src/a.py", &pins);
    assert_eq!(m, vec!["TypeError".to_string()]);
    assert!(s.contains("kept refs: TypeError"));
    let (s2, m2) = verify_summary("src/a.py TypeError", &pins);
    assert!(m2.is_empty());
    assert_eq!(s2, "src/a.py TypeError");
}

#[test]
fn error_signature_ignores_volatile_values() {
    assert_eq!(error_sig("Error: timeout after 31ms at /tmp/a/x.js"), error_sig("Error: timeout after 9000ms at /home/b/y.js"));
    assert_ne!(error_sig("Error: timeout"), error_sig("Error: refused"));
}

#[test]
fn redact_removes_credentials_only() {
    let t = "key sk-ant-abc123def456ghi789 and ghp_abcdefghijklmnop1234\nGEMINI_API_KEY=AQ.Ab8RN6K\nAuthorization: Bearer abcdefghijklmnop123\nrrn 900101-1234567\nsk-learn is a library, TOKEN=$TOKEN\nos.environ['API_KEY']\n-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----\n"; // repo-guard: allow a fake key, for the redaction test
    let (r, m) = redact(t);
    for gone in ["sk-ant-abc123", "ghp_abc", "AQ.Ab8RN6K", "abcdefghijklmnop123", "900101-1234567", "MIIE"] {
        assert!(!r.contains(gone), "{gone} leaked: {r}");
    }
    assert!(r.contains("sk-learn is a library"));
    assert!(r.contains("TOKEN=$TOKEN"));
    assert!(r.contains("os.environ"));
    assert!(m.len() >= 6);
}

#[test]
fn gate_is_silent_on_routine_and_caps_calls() {
    let g = |j: &str| run(&format!(r#"{{"op":"gate",{j}}}"#));
    assert!(!g(r#""trigger":"routine""#).get("consult").b());
    let p = g(r#""trigger":"plan""#);
    assert!(p.get("consult").b());
    let st1 = p.get("state").dump();
    assert!(!g(&format!(r#""trigger":"plan","state":{st1}"#)).get("consult").b(), "plan reviewed once");
    let e1 = g(&format!(r#""trigger":"error","error_sig":"x","state":{st1}"#));
    assert!(!e1.get("consult").b(), "first error stays silent");
    let e2 = g(&format!(r#""trigger":"error","error_sig":"x","state":{}"#, e1.get("state").dump()));
    assert!(e2.get("consult").b(), "same error twice consults");
    let e3 = g(&format!(r#""trigger":"error","error_sig":"x","state":{}"#, e2.get("state").dump()));
    assert!(!e3.get("consult").b(), "no second bill for the same error");
    let full = r#""state":{"calls":3}"#;
    assert!(!g(&format!(r#""trigger":"manual",{full}"#)).get("consult").b());
    assert!(!g(r#""trigger":"manual","state":{"tokens":7000}"#).get("consult").b());
}

#[test]
fn brief_obeys_cap_and_keeps_question() {
    let big = "word ".repeat(4000);
    let j = obj(vec![("op", st("brief")), ("cap", V::Num(300.0)), ("sections", V::Arr(vec![
        obj(vec![("name", st("QUESTION")), ("text", st("what next?"))]),
        obj(vec![("name", st("HISTORY")), ("text", st(&big))]),
        obj(vec![("name", st("GOAL")), ("text", st("ship"))]),
    ]))]);
    let b = run(&j.dump());
    assert!(b.get("brief").s().contains("what next?"));
    assert!(b.get("brief").s().contains("ship"));
    assert_eq!(b.get("dropped").arr()[0].s(), "HISTORY");
    assert!(b.get("tokens").n(0.0) <= 300.0);
}

fn start(models: &[&str], budget: f64) -> V {
    start_mode(models, budget, "auto")
}

fn start_mode(models: &[&str], budget: f64, mode: &str) -> V {
    let seats: Vec<V> = models.iter().enumerate().map(|(i, m)| obj(vec![("model", st(m)), ("title", st(&format!("S{i}"))), ("lens", V::Num(i as f64)), ("fallbacks", V::Arr(vec![]))])).collect();
    run(&obj(vec![("op", st("council_step")), ("start", obj(vec![("question", st("Postgres or SQLite?")), ("seats", V::Arr(seats)), ("options", V::Arr(vec![])), ("chair", obj(vec![("model", st("chair-model")), ("fallbacks", V::Arr(vec![]))])), ("limits", obj(vec![("max_tokens", V::Num(budget)), ("mode", st(mode))]))]))]).dump())
}

fn next(prev: &V, results: Vec<V>) -> V {
    run(&obj(vec![("op", st("council_step")), ("state", prev.get("state").clone()), ("results", V::Arr(results))]).dump())
}

fn card(id: &str, stance: &str, conf: u32) -> V {
    obj(vec![("id", st(id)), ("text", st(&format!(r#"```json
{{"stance":"{stance}","answer":"Use {stance}.","confidence":{conf},"reasons":["r"],"risks":["x"],"would_change":"load"}}
```"#))), ("in", V::Num(100.0)), ("out", V::Num(50.0))])
}

#[test]
fn prompts_have_no_persona_language() {
    let s = start(&["a", "b"], 0.0);
    for c in s.get("calls").arr() {
        let sys = c.get("system").s();
        assert!(sys.contains("No role-play"));
        for bad in ["You are", "High Table", "Arbiter", "Inquisitor", "Supreme", "Decree"] {
            assert!(!sys.contains(bad), "{bad} in prompt");
        }
    }
    assert!(!prompt(0).is_empty() && POOL[0].iter().take(40).any(|b| !b.is_ascii_alphabetic()));
}

#[test]
fn agreement_between_models_stops_early() {
    let s = start(&["a", "b", "c"], 0.0);
    let r = next(&s, vec![card("s0", "SQLite!", 90), card("s1", "sqlite", 80), card("s2", "sqlite", 75)]);
    assert!(r.get("done").b());
    assert_eq!(r.get("result").get("mode").s(), "agreed");
    assert_eq!(r.get("result").get("used").get("in").n(0.0), 300.0);
}

#[test]
fn copies_of_one_model_do_not_count_as_agreement() {
    let s = start(&["a", "a", "a"], 0.0);
    let r = next(&s, vec![card("s0", "sqlite", 90), card("s1", "sqlite", 90), card("s2", "sqlite", 90)]);
    assert!(!r.get("done").b());
    assert_eq!(r.get("calls").arr()[0].get("kind").s(), "review");
}

#[test]
fn review_is_anonymous_and_excludes_self() {
    let s = start(&["model-a", "model-b", "model-c"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 90), card("s2", "three", 90)]);
    let calls = r.get("calls").arr();
    assert_eq!(calls.len(), 3);
    let p = calls[0].get("prompt").s();
    for leak in ["model-a", "model-b", "model-c", "S0", "S1"] {
        assert!(!p.contains(leak), "{leak} leaked into review");
    }
    assert!(!p.contains("Use one."), "reviewer must not score itself");
    assert!(p.contains("Use two.") && p.contains("Use three."));
}

fn review(prev: &V, r: usize, score: impl Fn(&str) -> u32, fatal_for: Option<&str>) -> V {
    let map = prev.get("state").get("maps").arr()[r].arr();
    let mut scores = Vec::new();
    let mut fat = Vec::new();
    for m in map {
        let ci = m.get("card").n(0.0) as usize;
        let ans = prev.get("state").get("cards").arr()[ci].get("answer").s().to_string();
        let v = score(&ans) as f64;
        scores.push((m.get("label").s().to_string(), obj(vec![("evidence", V::Num(v)), ("risk", V::Num(v)), ("feasibility", V::Num(v))])));
        if fatal_for.is_some_and(|f| ans.contains(f)) {
            fat.push((m.get("label").s().to_string(), st("wrong")));
        }
    }
    let text = obj(vec![("scores", V::Obj(scores)), ("fatal", V::Obj(fat))]).dump();
    obj(vec![("id", st(&format!("r{r}"))), ("text", st(&text)), ("in", V::Num(10.0)), ("out", V::Num(10.0))])
}

#[test]
fn clear_winner_needs_no_chair_and_matrix_is_correct() {
    let s = start(&["a", "b", "c", "d"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 80), card("s2", "three", 70), card("s3", "four", 60)]);
    let score = |a: &str| if a.contains("two") { 5 } else if a.contains("one") { 3 } else { 1 };
    let rv: Vec<V> = (0..4).map(|i| review(&r, i, score, None)).collect();
    let d = next(&r, rv);
    assert!(d.get("done").b());
    let res = d.get("result");
    assert_eq!(res.get("mode").s(), "ranked");
    assert!(res.get("final").s().contains("Use two."));
    let m = res.get("matrix").arr();
    assert_eq!(m[0].get("seat").n(0.0), 2.0);
    assert_eq!(m[0].get("evidence").n(0.0), 5.0);
}

#[test]
fn fatal_flaw_or_tie_goes_to_chair_with_short_material() {
    let s = start(&["a", "b", "c"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 80), card("s2", "three", 70)]);
    let rv: Vec<V> = (0..3).map(|i| review(&r, i, |_| 3, Some("Use one"))).collect();
    let d = next(&r, rv);
    assert!(!d.get("done").b());
    let c = &d.get("calls").arr()[0];
    assert_eq!(c.get("model").s(), "chair-model");
    assert!(c.get("system").s().contains("Do not average"));
    let f = next(&d, vec![obj(vec![("id", st("chair")), ("text", st("**Decision:** two")), ("in", V::Num(5.0)), ("out", V::Num(5.0))])]);
    assert_eq!(f.get("result").get("mode").s(), "resolved");
    assert_eq!(f.get("result").get("final").s(), "**Decision:** two");
}

#[test]
fn budget_stops_with_best_answer_and_says_so() {
    let s = start(&["a", "b", "c", "d"], 500.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 80), card("s2", "three", 70), card("s3", "four", 60)]);
    assert!(r.get("done").b());
    let res = r.get("result");
    assert_eq!(res.get("mode").s(), "budget");
    assert!(res.get("final").s().contains("Use one."));
    assert!(res.get("final").s().contains("not cross-checked"));
}

#[test]
fn survives_garbage_and_failures() {
    let s = start(&["a", "b", "c"], 0.0);
    let bad = obj(vec![("id", st("s0")), ("text", st("I think SQLite, honestly."))]);
    let err = obj(vec![("id", st("s1")), ("error", st("429"))]);
    let r = next(&s, vec![bad, err, card("s2", "sqlite", 90)]);
    assert!(!r.get("done").b(), "one valid and one unformatted answer still get scored");
    assert_eq!(r.get("calls").arr().len(), 2);
    let d = next(&r, vec![obj(vec![("id", st("r0")), ("text", st("not json"))]), obj(vec![("id", st("r1")), ("text", st("{}"))])]);
    assert!(!d.get("done").b(), "no usable scores goes to the chair");
    let none = next(&start(&["a", "b"], 0.0), vec![obj(vec![("id", st("s0")), ("error", st("x"))]), obj(vec![("id", st("s1")), ("error", st("y"))])]);
    assert_eq!(none.get("result").get("mode").s(), "failed");
    assert_eq!(run("not json").get("error").s(), "kernel: invalid JSON input");
}

#[test]
fn stages_cost_far_less_than_full_text_review() {
    // Same token assumptions as the N x N design: question 500, answer 800, ranking text 400.
    let (n, q, a, rk) = (4usize, 500usize, 800usize, 400usize);
    let full = n * (q + a) + n * (q + n * a + rk) + (q + n * a + n * rk + a);
    assert!(worst_case(&"x".repeat(q * 4), n) < full);
}

#[test]
fn dial_fast_stops_after_answers_and_says_so() {
    let s = start_mode(&["a", "b"], 0.0, "fast");
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 40)]);
    assert!(r.get("done").b());
    assert_eq!(r.get("result").get("mode").s(), "fast");
    assert!(r.get("result").get("final").s().contains("Use one."));
    assert!(r.get("result").get("final").s().contains("not cross-checked"));
}

#[test]
fn dial_thorough_never_stops_early() {
    let s = start_mode(&["a", "b", "c"], 0.0, "thorough");
    let r = next(&s, vec![card("s0", "x", 95), card("s1", "x", 95), card("s2", "x", 95)]);
    assert!(!r.get("done").b(), "agreement does not skip scoring");
    let score = |a: &str| if a.contains("x") { 5 } else { 1 };
    let rv: Vec<V> = (0..3).map(|i| review(&r, i, score, None)).collect();
    let d = next(&r, rv);
    assert!(!d.get("done").b(), "a clear winner does not skip the final step");
    assert_eq!(d.get("calls").arr()[0].get("kind").s(), "chair");
}

#[test]
fn prompts_name_the_language_and_the_labels() {
    let s = run(&obj(vec![("op", st("council_step")), ("start", obj(vec![("question", st("어떤 DB를 쓸까?")), ("seats", V::Arr(vec![
        obj(vec![("model", st("a")), ("title", st("A"))]), obj(vec![("model", st("b")), ("title", st("B"))]), obj(vec![("model", st("c")), ("title", st("C"))])])),
        ("chair", obj(vec![("model", st("z"))])), ("limits", obj(vec![]))]))]).dump());
    assert!(s.get("calls").arr()[0].get("system").s().contains("in Korean"));
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 90), card("s2", "three", 90)]);
    assert!(r.get("calls").arr()[0].get("system").s().contains("one entry for each of: A, B."));
}

#[test]
fn unscored_answer_is_blank_not_zero() {
    let s = start(&["a", "b", "c"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 90), card("s2", "three", 90)]);
    let d = next(&r, vec![obj(vec![("id", st("r0")), ("text", st("garbage"))]), obj(vec![("id", st("r1")), ("text", st("garbage"))]), obj(vec![("id", st("r2")), ("text", st("garbage"))])]);
    assert_eq!(d.get("state").get("matrix").arr()[0].get("evidence"), &V::Null);
}

#[test]
fn redact_covers_personal_data_and_leaves_ordinary_numbers() {
    let (r, m) = redact("mail a.b+c@ex-ample.co.kr card 4111 1111 1111 1111 ssn 123-45-6789 phone 010-1234-5678 us 415-555-0199");
    for gone in ["a.b+c@", "4111", "123-45-6789", "010-1234", "415-555"] {
        assert!(!r.contains(gone), "{gone} leaked: {r}");
    }
    assert_eq!(m.len(), 5);
    let keep = "port 8080 on 2026-11-07 v1.2.3 pid 1234567890123 at foo@bar line 10 id 12-34 한글 uses user@host";
    let (r2, m2) = redact(keep);
    assert_eq!(r2, keep);
    assert!(m2.is_empty());
}

#[test]
fn partial_scoring_never_claims_a_clear_winner() {
    let s = start(&["a", "b", "c"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 90), card("s2", "three", 90)]);
    // Weak models fill in only the example label: every reviewer scored one answer of two.
    let partial = |i: usize| obj(vec![("id", st(&format!("r{i}"))), ("text", st(r#"{"scores":{"A":{"evidence":5,"risk":5,"feasibility":5}}}"#))]);
    let d = next(&r, vec![partial(0), partial(1), partial(2)]);
    assert!(!d.get("done").b(), "must go to the final step");
    assert_eq!(d.get("calls").arr()[0].get("kind").s(), "chair");
}

#[test]
fn two_answers_cannot_produce_a_clear_winner() {
    // Each reviews only the other, so each is "first" once and nothing is learned.
    let s = start(&["a", "b"], 0.0);
    let r = next(&s, vec![card("s0", "one", 90), card("s1", "two", 90)]);
    let rv: Vec<V> = (0..2).map(|i| review(&r, i, |_| 5, None)).collect();
    assert!(!next(&r, rv).get("done").b());
}
