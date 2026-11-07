//! Loss-minimal compression. Runs on every turn and costs no tokens.
//! Anything removed from a prompt is returned as a stub with its original bytes, so the host can
//! store it by id and restore it exactly. Identifiers are pinned so an LLM summary cannot drop them.
use crate::json::{obj, st, V};
use alloc::{format, string::{String, ToString}, vec, vec::Vec};

pub fn estimate(t: &str) -> usize {
    if t.is_empty() {
        return 0;
    }
    let (mut cjk, mut other) = (0usize, 0usize);
    for c in t.chars() {
        let u = c as u32;
        if (0x1100..=0x11ff).contains(&u) || (0x3040..=0x30ff).contains(&u) || (0x3130..=0x318f).contains(&u)
            || (0x3400..=0x9fff).contains(&u) || (0xac00..=0xd7af).contains(&u) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    other / 4 + cjk * 2 / 3 + 1
}

pub fn hash_id(t: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in t.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

fn digits(b: &[u8]) -> bool {
    !b.is_empty() && b.iter().all(u8::is_ascii_digit)
}

fn level_prefix(t: &str) -> bool {
    for l in ["DEBUG", "INFO", "WARNING", "WARN", "ERROR", "TRACE"] {
        if let Some(r) = t.strip_prefix(l) {
            if r.starts_with(':') || r.starts_with(' ') {
                return true;
            }
        }
        if let Some(r) = t.strip_prefix('[') {
            if r.strip_prefix(l).is_some_and(|x| x.starts_with(']')) {
                return true;
            }
        }
    }
    false
}

fn is_log(l: &str) -> bool {
    let t = l.trim_start();
    if t.is_empty() {
        return false;
    }
    let b = t.as_bytes();
    (t.starts_with("at ") && t.contains('('))
        || (t.starts_with("File \"") && t.contains(", line "))
        || t.starts_with("Traceback (most recent")
        || (b.len() >= 10 && digits(&b[..4]) && b[4] == b'-' && digits(&b[5..7]) && b[7] == b'-' && digits(&b[8..10]))
        || level_prefix(t)
        || (t.starts_with('#') && t.contains(" 0x"))
        || l.len() - t.len() >= 4
}

fn is_error(l: &str) -> bool {
    let low = l.to_ascii_lowercase();
    ["error", "exception", "panic", "failed", "fatal", "traceback"].iter().any(|k| low.contains(k))
}

/// Replaces volatile values, so the same failure in two runs has the same signature.
pub fn normalize(l: &str) -> String {
    let mut o = String::new();
    let mut last = ' ';
    for c in l.trim().chars() {
        let d = if c.is_ascii_digit() { '#' } else { c };
        if !(d == '#' && last == '#') {
            o.push(d);
        }
        last = d;
    }
    o
}

pub fn error_sig(t: &str) -> String {
    let line = t.lines().rev().find(|l| is_error(l)).unwrap_or_else(|| t.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(""));
    let mut n = String::new();
    for w in normalize(line).split_whitespace() {
        // Paths and hex addresses change between runs and machines.
        let w = if w.contains('/') || w.starts_with("0x") { "<ref>" } else { w };
        n.push_str(w);
        n.push(' ');
    }
    hash_id(&n)
}

fn defs(body: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in body {
        let t = l.trim_start();
        for k in ["pub async fn ", "pub fn ", "async fn ", "fn ", "async def ", "def ", "export function ", "function ", "class ", "struct ", "enum ", "func ", "interface ", "type "] {
            if let Some(r) = t.strip_prefix(k) {
                let id: String = r.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !id.is_empty() && !out.contains(&id) && out.len() < 8 {
                    out.push(id);
                }
                break;
            }
        }
    }
    out
}

pub struct Out {
    pub text: String,
    pub stubs: Vec<V>,
}

fn stub(body: &str, kind: &str, stubs: &mut Vec<V>) -> String {
    let id = hash_id(body);
    if !stubs.iter().any(|s| s.get("id").s() == id) {
        stubs.push(obj(vec![("id", st(&id)), ("kind", st(kind)), ("body", st(body))]));
    }
    id
}

pub fn compress(input: &str, code_max: usize, log_max: usize) -> Out {
    let text = input.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut stubs = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    let mut blanks = 0;
    while i < lines.len() {
        let l = lines[i];
        if l.trim_start().starts_with("```") {
            let lang = l.trim_start().trim_start_matches('`').trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            let end = j.min(lines.len() - 1);
            let n = j - i - 1;
            let raw = lines[i..=end].join("\n");
            let key = hash_id(&raw);
            if let Some((_, first)) = seen.iter().find(|(k, _)| *k == key) {
                out.push(format!("[same as {first}]"));
            } else if n > code_max {
                let id = stub(&raw, "code", &mut stubs);
                let d = defs(&lines[i + 1..j]);
                let kind = if lang.is_empty() { String::from("code") } else { format!("code:{lang}") };
                let tail = if d.is_empty() { String::new() } else { format!("; defs: {}", d.join(", ")) };
                out.push(format!("[#{id} {kind} {n} lines{tail}]"));
                seen.push((key, format!("#{id}")));
            } else {
                out.push(raw.clone());
                if n >= 3 {
                    let h = hash_id(&raw);
                    seen.push((key, format!("earlier block {}", &h[..6])));
                }
            }
            i = end + 1;
            blanks = 0;
            continue;
        }
        if is_log(l) {
            let mut j = i;
            while j < lines.len() && is_log(lines[j]) && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            let run = &lines[i..j];
            if run.len() > log_max {
                // Collapse repeats first. Lines that differ only in numbers are the same event.
                let mut uniq: Vec<(String, &str, usize)> = Vec::new();
                for r in run {
                    let n = normalize(r);
                    if let Some(u) = uniq.iter_mut().find(|u| u.0 == n) {
                        u.2 += 1;
                    } else {
                        uniq.push((n, r, 1));
                    }
                }
                let id = stub(&run.join("\n"), "log", &mut stubs);
                out.push(format!("[#{id} log {} lines, {} distinct]", run.len(), uniq.len()));
                let keep: Vec<&(String, &str, usize)> = if uniq.len() <= log_max {
                    uniq.iter().collect()
                } else {
                    let mut k: Vec<&(String, &str, usize)> = vec![&uniq[0]];
                    for u in uniq.iter().filter(|u| is_error(u.1)).take(log_max.saturating_sub(2)) {
                        if !k.iter().any(|x| x.0 == u.0) {
                            k.push(u);
                        }
                    }
                    let last = &uniq[uniq.len() - 1];
                    if !k.iter().any(|x| x.0 == last.0) {
                        k.push(last);
                    }
                    k
                };
                for u in keep {
                    out.push(if u.2 > 1 { format!("{} (x{})", u.1, u.2) } else { u.1.to_string() });
                }
            } else {
                out.extend(run.iter().map(|r| r.trim_end().to_string()));
            }
            i = j;
            blanks = 0;
            continue;
        }
        if l.trim().is_empty() {
            blanks += 1;
            if blanks < 2 {
                out.push(String::new());
            }
        } else {
            blanks = 0;
            out.push(l.trim_end().to_string());
        }
        i += 1;
    }
    Out { text: out.join("\n"), stubs }
}

const EXT: &[&str] = &[".py", ".rs", ".ts", ".js", ".mjs", ".tsx", ".go", ".c", ".h", ".cpp", ".java", ".kt", ".md", ".toml", ".json", ".yaml", ".yml", ".sh", ".html", ".css", ".sql", ".wasm", ".lock", ".txt"];
const SKIP: &[&str] = &["if", "for", "while", "switch", "return", "print", "function", "catch", "and", "or", "not", "e.g", "i.e"];

/// The facts a summary may not lose: paths, calls, versions, hashes, error names, stub ids, figures.
pub fn anchors(t: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let add = |s: &str, out: &mut Vec<String>| {
        if s.len() >= 2 && s.len() <= 140 && !out.iter().any(|x| x == s) && out.len() < max {
            out.push(s.to_string());
        }
    };
    for line in t.lines() {
        let tl = line.trim();
        if tl.starts_with("[#") {
            if let Some(id) = tl.get(1..14) {
                add(id, &mut out);
            }
        }
        if is_error(tl) && tl.len() <= 140 && !tl.starts_with('[') {
            add(tl, &mut out);
        }
    }
    for raw in t.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '`' | '"' | '\'' | '<' | '>' | '[' | ']' | '{' | '}')) {
        let w = raw.trim_matches(|c: char| matches!(c, '.' | ':' | '!' | '?' | '*' | '=' ) );
        if w.is_empty() {
            continue;
        }
        let bare = w.split('(').next().unwrap_or(w).trim_matches(')');
        let b = bare.as_bytes();
        let alpha = b.iter().any(u8::is_ascii_alphabetic);
        let digit = b.iter().any(u8::is_ascii_digit);
        let is_path = (bare.contains('/') && alpha && bare.len() > 3) || EXT.iter().any(|e| bare.len() > e.len() && bare.ends_with(e));
        let is_call = w.contains('(') && alpha && bare.len() >= 3 && !SKIP.contains(&bare)
            && bare.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':');
        let is_ident = alpha && bare.chars().all(|c| c.is_alphanumeric() || c == '_') && (bare.contains('_')
            || bare.ends_with("Error") || bare.ends_with("Exception"));
        let is_ver = digit && bare.contains('.') && bare.trim_start_matches('v').bytes().all(|c| c.is_ascii_digit() || c == b'.');
        let is_hex = b.len() >= 7 && alpha && digit && b.iter().all(u8::is_ascii_hexdigit);
        let is_fig = digit && b.len() >= 2 && (b.iter().all(|c| c.is_ascii_digit() || matches!(c, b'$' | b'%' | b'.')) || (digits(&b[..b.len() - 1]) && matches!(b[b.len() - 1], b'k' | b'K' | b'M' | b'%')));
        if is_path || is_call || is_ident || is_ver || is_hex || is_fig {
            add(bare, &mut out);
        }
    }
    out
}

pub fn verify_summary(summary: &str, pins: &[V]) -> (String, Vec<String>) {
    let missing: Vec<String> = pins.iter().map(|p| p.s()).filter(|p| !p.is_empty() && !summary.contains(p)).map(String::from).collect();
    if missing.is_empty() {
        return (summary.to_string(), missing);
    }
    (format!("{}\n- kept refs: {}", summary.trim_end(), missing.join(" | ")), missing)
}

fn tokenlike(b: &[u8]) -> bool {
    b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/' | b'+' | b'='))
}

/// Removes credentials and national ids before a prompt leaves the machine. Reversible by map.
pub fn redact(t: &str) -> (String, Vec<V>) {
    const PREFIX: &[&str] = &["sk-ant-", "sk-or-", "sk-", "ghp_", "gho_", "ghs_", "github_pat_", "glpat-", "AIza", "xoxb-", "xoxp-", "AKIA", "hf_", "gsk_"];
    const NAMES: &[&str] = &["API_KEY", "APIKEY", "API-KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "PRIVATE_KEY"];
    let mut map: Vec<V> = Vec::new();
    let put = |secret: &str, map: &mut Vec<V>| -> String {
        if let Some(m) = map.iter().find(|m| m.get("value").s() == secret) {
            return m.get("mark").s().to_string();
        }
        let mark = format!("[REDACTED_{}]", map.len() + 1);
        map.push(obj(vec![("mark", st(&mark)), ("value", st(secret))]));
        mark
    };
    let mut o = String::new();
    let mut in_key = String::new();
    for line in t.split_inclusive('\n') {
        let tl = line.trim();
        if !in_key.is_empty() || (tl.starts_with("-----BEGIN") && tl.contains("PRIVATE KEY")) {
            in_key.push_str(line);
            if tl.starts_with("-----END") {
                let m = put(in_key.trim_end(), &mut map);
                o.push_str(&m);
                o.push('\n');
                in_key.clear();
            }
            continue;
        }
        let mut pos = 0;
        let b = line.as_bytes();
        while pos < b.len() {
            let at_start = pos == 0 || !(b[pos - 1].is_ascii_alphanumeric() || matches!(b[pos - 1], b'_' | b'-'));
            if at_start && b[pos].is_ascii_alphanumeric() {
                let mut e = pos;
                while e < b.len() && tokenlike(&b[e..e + 1]) {
                    e += 1;
                }
                let w = &line[pos..e];
                let w = w.trim_end_matches(['.', '/', '=']);
                let wl = w.len();
                let pre = PREFIX.iter().any(|p| w.starts_with(p) && wl >= p.len() + 12 && w.bytes().any(|c| c.is_ascii_digit()));
                let wb = w.as_bytes();
                let rrn = wl == 14 && wb[6] == b'-' && digits(&wb[..6]) && digits(&wb[7..]) && matches!(wb[7], b'1'..=b'8');
                if pre || rrn {
                    o.push_str(&put(w, &mut map));
                    pos += wl;
                    continue;
                }
                o.push_str(&line[pos..pos + wl.max(1)]);
                pos += wl.max(1);
                continue;
            }
            let ch = line[pos..].chars().next().unwrap();
            o.push(ch);
            pos += ch.len_utf8();
        }
    }
    if !in_key.is_empty() {
        o.push_str(&put(in_key.trim_end(), &mut map));
    }
    // NAME=value and "Bearer x" forms: the value can have any shape.
    let mut res = String::new();
    for line in o.split_inclusive('\n') {
        let up = line.to_ascii_uppercase();
        let named = NAMES.iter().filter_map(|n| up.find(n).map(|i| i + n.len())).min();
        let mut done = false;
        if let Some(mut i) = named {
            let b = line.as_bytes();
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let mut j = i;
            while j < b.len() && matches!(b[j], b' ' | b'"' | b'\'') {
                j += 1;
            }
            if j < b.len() && matches!(b[j], b'=' | b':') {
                j += 1;
                while j < b.len() && matches!(b[j], b' ' | b'"' | b'\'') {
                    j += 1;
                }
                let mut e = j;
                while e < b.len() && !matches!(b[e], b' ' | b'"' | b'\'' | b'\n' | b',' | b';') {
                    e += 1;
                }
                let v = &line[j..e];
                if v.len() >= 8 && !v.starts_with("[REDACTED") && !v.starts_with('$') && !v.starts_with("os.") && !v.starts_with("process.") {
                    let m = put(v, &mut map);
                    res.push_str(&line[..j]);
                    res.push_str(&m);
                    res.push_str(&line[e..]);
                    done = true;
                }
            }
        }
        if !done {
            if let Some(i) = line.find("Bearer ") {
                let s = i + 7;
                let e = line[s..].find(|c: char| c.is_whitespace() || c == '"').map_or(line.len(), |x| s + x);
                if e - s >= 12 && !line[s..e].starts_with("[REDACTED") {
                    let m = put(&line[s..e], &mut map);
                    res.push_str(&line[..s]);
                    res.push_str(&m);
                    res.push_str(&line[e..]);
                    done = true;
                }
            }
        }
        if !done {
            res.push_str(line);
        }
    }
    let out = pii(&res, &mut map);
    (out, map)
}

fn luhn(d: &[u8]) -> bool {
    let (mut sum, mut alt) = (0u32, false);
    for c in d.iter().rev() {
        let mut n = (*c - b'0') as u32;
        if alt {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        alt = !alt;
    }
    sum % 10 == 0
}

fn group_lens(s: &str) -> Vec<usize> {
    s.split(['-', ' ']).map(str::len).collect()
}

/// Which kind of personal number this digit group is. Dates, versions and ports match nothing.
fn number_kind(s: &str) -> Option<&'static str> {
    let d: Vec<u8> = s.bytes().filter(u8::is_ascii_digit).collect();
    let g = group_lens(s);
    let dashed = s.contains('-') && !s.contains(' ');
    if (13..=19).contains(&d.len()) && luhn(&d) && (g.len() == 1 || g.iter().all(|x| *x >= 3)) {
        return Some("CARD");
    }
    if dashed && g == [3, 2, 4] && &d[..3] != b"000" && &d[..3] != b"666" && d[0] != b'9' && &d[3..5] != b"00" && &d[5..] != b"0000" {
        return Some("SSN");
    }
    if dashed && d.len() >= 9 && d.len() <= 11 && g.len() == 3 {
        let kr = d[0] == b'0' && (g[0] == 2 || g[0] == 3) && (g[1] == 3 || g[1] == 4) && g[2] == 4;
        let us = g == [3, 3, 4] && d[0] != b'0' && d[0] != b'1';
        if kr || us {
            return Some("PHONE");
        }
    }
    None
}

/// Same categories as sentinel-kit's local redactor, here so every host gets them, browsers included.
fn pii(t: &str, map: &mut Vec<V>) -> String {
    let b = t.as_bytes();
    let mut o = String::with_capacity(t.len());
    let mut i = 0;
    let mark = |value: &str, kind: &str, map: &mut Vec<V>| -> String {
        let m = format!("[REDACTED_{}_{}]", kind, map.len() + 1);
        map.push(obj(vec![("mark", st(&m)), ("value", st(value))]));
        m
    };
    let word = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-');
    while i < b.len() {
        let c = b[i];
        let boundary = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || matches!(b[i - 1], b'.' | b'-' | b'_' | b'+' | b'/' | b'#' | b'@'));
        if boundary && word(c) && c != b'.' {
            let mut e = i;
            while e < b.len() && word(b[e]) {
                e += 1;
            }
            if e < b.len() && b[e] == b'@' {
                let mut z = e + 1;
                while z < b.len() && (b[z].is_ascii_alphanumeric() || matches!(b[z], b'.' | b'-')) {
                    z += 1;
                }
                let dom = t[e + 1..z].trim_end_matches(['.', '-']);
                let tld = dom.rsplit('.').next().unwrap_or("");
                if dom.contains('.') && tld.len() >= 2 && tld.bytes().all(|x| x.is_ascii_alphabetic()) {
                    let end = e + 1 + dom.len();
                    o.push_str(&mark(&t[i..end], "EMAIL", map));
                    i = end;
                    continue;
                }
            }
        }
        if boundary && c.is_ascii_digit() {
            let mut e = i;
            while e < b.len() && (b[e].is_ascii_digit() || (matches!(b[e], b'-' | b' ') && e + 1 < b.len() && b[e + 1].is_ascii_digit() && e - i < 24)) {
                e += 1;
            }
            let after = e >= b.len() || !(b[e].is_ascii_alphanumeric() || matches!(b[e], b'-' | b'_'));
            if after {
                if let Some(kind) = number_kind(&t[i..e]) {
                    o.push_str(&mark(&t[i..e], kind, map));
                    i = e;
                    continue;
                }
            }
            o.push_str(&t[i..e]);
            i = e;
            continue;
        }
        let ch = t[i..].chars().next().unwrap();
        o.push(ch);
        i += ch.len_utf8();
    }
    o
}
