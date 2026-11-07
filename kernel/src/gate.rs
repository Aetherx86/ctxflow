//! Decides whether the expensive model is allowed to read anything. Routine turns stay silent.
use crate::compress::{compress, estimate};
use crate::json::{obj, st, V};
use alloc::{format, string::{String, ToString}, vec, vec::Vec};

pub fn gate(i: &V) -> V {
    let trigger = i.get("trigger").s();
    let mut state = match i.get("state") {
        V::Obj(_) => i.get("state").clone(),
        _ => V::Obj(Vec::new()),
    };
    let l = i.get("limits");
    let max_calls = l.get("max_calls").n(3.0);
    let max_tokens = l.get("max_tokens").n(6000.0);
    let repeat = l.get("error_repeat").n(2.0);
    let calls = state.get("calls").n(0.0);
    let spent = state.get("tokens").n(0.0);
    let sig = i.get("error_sig").s().to_string();
    let mut errors = match state.get("errors") {
        V::Obj(_) => state.get("errors").clone(),
        _ => V::Obj(Vec::new()),
    };
    let mut consult = false;
    let reason: String;
    if trigger == "error" {
        let n = errors.get(&sig).n(0.0) + 1.0;
        errors.set(&sig, V::Num(n));
    }
    if calls >= max_calls && trigger != "routine" {
        reason = format!("blocked: {calls} of {max_calls} advisor calls used");
    } else if spent >= max_tokens && trigger != "routine" {
        reason = format!("blocked: advisor token budget used ({spent}/{max_tokens})");
    } else {
        match trigger {
            "plan" if !state.get("planned").b() => {
                consult = true;
                state.set("planned", V::Bool(true));
                reason = "before the plan".to_string();
            }
            "plan" => reason = "silent: plan already reviewed".to_string(),
            "error" => {
                let n = errors.get(&sig).n(0.0);
                let key = format!("c:{sig}");
                if n >= repeat && !errors.get(&key).b() {
                    consult = true;
                    errors.set(&key, V::Bool(true));
                    reason = format!("same error seen {n} times");
                } else if n >= repeat {
                    reason = "silent: advice for this error already given".to_string();
                } else {
                    reason = format!("silent: error seen {n} time(s)");
                }
            }
            "done" if i.get("has_changes").b() => {
                consult = true;
                reason = "before done".to_string();
            }
            "done" => reason = "silent: nothing changed since the last review".to_string(),
            "manual" => {
                consult = true;
                reason = "requested".to_string();
            }
            _ => reason = "silent: routine turn".to_string(),
        }
    }
    state.set("errors", errors);
    if consult {
        state.set("calls", V::Num(calls + 1.0));
    }
    obj(vec![("consult", V::Bool(consult)), ("reason", st(&reason)), ("state", state)])
}

/// Sections arrive in priority order. The first always stays; later ones go when the cap is hit.
pub fn brief(i: &V) -> V {
    let cap = i.get("cap").n(1500.0) as usize;
    let mut parts: Vec<String> = Vec::new();
    let mut stubs: Vec<V> = Vec::new();
    let mut dropped: Vec<V> = Vec::new();
    let mut used = 0;
    for (n, s) in i.get("sections").arr().iter().enumerate() {
        let body = s.get("text").s();
        if body.trim().is_empty() {
            continue;
        }
        let c = compress(body, 8, 4);
        let block = format!("=== {} ===\n{}", s.get("name").s(), c.text.trim());
        let t = estimate(&block);
        if n > 0 && used + t > cap {
            dropped.push(st(s.get("name").s()));
            continue;
        }
        used += t;
        parts.push(block);
        stubs.extend(c.stubs);
    }
    obj(vec![("brief", st(&parts.join("\n\n"))), ("tokens", V::Num(used as f64)), ("dropped", V::Arr(dropped)), ("stubs", V::Arr(stubs))])
}
