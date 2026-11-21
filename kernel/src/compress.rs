//! Loss-minimal compression. Runs on every turn and costs no tokens.
//! Anything removed from a prompt is returned as a stub with its original bytes, so the host can
//! store it by id and restore it exactly. Identifiers are pinned so an LLM summary cannot drop them.
use crate::json::{obj, st, V};
use alloc::collections::{BTreeMap, BTreeSet};
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

/// Characters a reader cannot see but a model or terminal acts on: Trojan Source bidi controls,
/// Unicode tag characters and variation selectors used to smuggle hidden text, invisible fillers,
/// and terminal control codes. Newline, carriage return and tab stay.
pub fn is_hidden(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F..=0x9F
        | 0xAD | 0x34F | 0x61C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F
        | 0x200B | 0x200E | 0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0x3164 | 0xFE00..=0xFE0D | 0xFEFF | 0xFFA0 | 0xFFF9..=0xFFFB
        | 0xE0000..=0xE007F | 0xE0100..=0xE01EF)
}

pub fn sanitize(t: &str) -> (String, usize) {
    let mut removed = 0;
    let out: String = t.chars().filter(|c| {
        let hide = is_hidden(*c);
        removed += hide as usize;
        !hide
    }).collect();
    (out, removed)
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
    for l in ["DEBUG", "INFO", "WARNING", "WARN", "ERROR", "FATAL", "CRITICAL", "CRIT", "TRACE"] {
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
        || (t.starts_with("thread '") && t.contains("panicked at"))
        || t.starts_with("stack backtrace:")
        || (t.starts_with("goroutine ") && t.contains('['))
        || t.starts_with("--- FAIL:")
        || t.starts_with("Caused by:")
        || (b.len() >= 10 && digits(&b[..4]) && b[4] == b'-' && digits(&b[5..7]) && b[7] == b'-' && digits(&b[8..10]))
        || level_prefix(t)
        || (t.starts_with('#') && t.contains(" 0x"))
        || l.len() - t.len() >= 4
}

fn is_error(l: &str) -> bool {
    let low = l.to_ascii_lowercase();
    ["error", "exception", "panic", "failed", "fatal", "critical", "traceback"].iter().any(|k| low.contains(k))
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

fn stub(body: &str, kind: &str, stubs: &mut Vec<V>, ids: &mut BTreeSet<String>) -> String {
    let id = hash_id(body);
    if ids.insert(id.clone()) {
        stubs.push(obj(vec![("id", st(&id)), ("kind", st(kind)), ("body", st(body))]));
    }
    id
}

pub fn compress(input: &str, code_max: usize, log_max: usize) -> Out {
    let text = input.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut stubs = Vec::new();
    let mut ids = BTreeSet::new();
    // Indexed, so a hostile input with many blocks or log lines stays linear-logarithmic.
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
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
            if let Some(first) = seen.get(&key) {
                out.push(format!("[same as {first}]"));
            } else if n > code_max {
                let id = stub(&raw, "code", &mut stubs, &mut ids);
                let d = defs(&lines[i + 1..j]);
                let kind = if lang.is_empty() { String::from("code") } else { format!("code:{lang}") };
                let tail = if d.is_empty() { String::new() } else { format!("; defs: {}", d.join(", ")) };
                out.push(format!("[#{id} {kind} {n} lines{tail}]"));
                seen.insert(key, format!("#{id}"));
            } else {
                out.push(raw.clone());
                if n >= 3 {
                    let h = hash_id(&raw);
                    seen.insert(key, format!("earlier block {}", &h[..6]));
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
                let mut index: BTreeMap<String, usize> = BTreeMap::new();
                for r in run {
                    let n = normalize(r);
                    match index.get(&n) {
                        Some(&i) => uniq[i].2 += 1,
                        None => {
                            index.insert(n.clone(), uniq.len());
                            uniq.push((n, r, 1));
                        }
                    }
                }
                let id = stub(&run.join("\n"), "log", &mut stubs, &mut ids);
                out.push(format!("[#{id} log {} lines, {} distinct]", run.len(), uniq.len()));
                let keep: Vec<&(String, &str, usize)> = if uniq.len() <= log_max {
                    uniq.iter().collect()
                } else {
                    let mut k: Vec<&(String, &str, usize)> = vec![&uniq[0]];
                    for u in uniq.iter().skip(1).filter(|u| is_error(u.1)).take(log_max.saturating_sub(2)) {
                        k.push(u);
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
    let mut have: BTreeSet<String> = BTreeSet::new();
    let mut add = |s: &str, out: &mut Vec<String>| {
        if s.len() >= 2 && s.len() <= 140 && out.len() < max && have.insert(s.to_string()) {
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

fn put_secret(secret: &str, kind: Option<&str>, map: &mut Vec<V>) -> String {
    if let Some(m) = map.iter().find(|m| m.get("value").s() == secret) {
        return m.get("mark").s().to_string();
    }
    let mark = match kind {
        Some(k) => format!("[REDACTED_{}_{}]", k, map.len() + 1),
        None => format!("[REDACTED_{}]", map.len() + 1),
    };
    map.push(obj(vec![("mark", st(&mark)), ("value", st(secret))]));
    mark
}

/// Removes credentials and national ids before a prompt leaves the machine. Reversible by map.
pub fn redact(t: &str) -> (String, Vec<V>) {
    const PREFIX: &[&str] = &[
        "sk-ant-", "sk-or-", "sk-", "sk_live_", "rk_live_", "pk_live_", "whsec_", "ghp_", "gho_",
        "ghs_", "ghu_", "ghr_", "github_pat_", "glpat-", "glrt-", "AIza", "ya29.", "xoxb-",
        "xoxp-", "xoxa-", "xoxr-", "xoxe-", "xapp-", "AKIA", "ASIA", "hf_", "gsk_", "npm_",
        "pypi-", "dop_v1_", "shpat_", "sq0atp-", "SG.", "sbp_", "eyJ",
    ];
    const NAMES: &[&str] = &[
        "API_KEY", "APIKEY", "API-KEY", "ACCESS_KEY", "SIGNING_KEY", "TOKEN", "SECRET",
        "PASSWORD", "PASSWD", "PRIVATE_KEY", "PRIVATE-KEY", "CREDENTIAL",
    ];
    let mut map: Vec<V> = Vec::new();
    let mut o = String::new();
    let mut in_key = String::new();
    for line in t.split_inclusive('\n') {
        let tl = line.trim();
        if !in_key.is_empty() || (tl.starts_with("-----BEGIN") && tl.contains("PRIVATE KEY")) {
            in_key.push_str(line);
            if tl.starts_with("-----END") {
                let m = put_secret(in_key.trim_end(), None, &mut map);
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
                    o.push_str(&put_secret(w, None, &mut map));
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
        o.push_str(&put_secret(in_key.trim_end(), None, &mut map));
    }
    // NAME=value forms: scan every match on each line so a second secret on the same line never leaks.
    let mut named_out = String::with_capacity(o.len());
    for line in o.split_inclusive('\n') {
        let up = line.to_ascii_uppercase();
        let b = line.as_bytes();
        let mut cursor = 0;
        while cursor < b.len() {
            let next = NAMES
                .iter()
                .filter_map(|n| up[cursor..].find(n).map(|k| (cursor + k, cursor + k + n.len())))
                .min_by_key(|x| x.0);
            let Some((_, mut i)) = next else {
                named_out.push_str(&line[cursor..]);
                break;
            };
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let mut j = i;
            while j < b.len() && matches!(b[j], b' ' | b'"' | b'\'') {
                j += 1;
            }
            let mut replaced = false;
            if j < b.len() && matches!(b[j], b'=' | b':') {
                j += 1;
                while j < b.len() && matches!(b[j], b' ' | b'"' | b'\'') {
                    j += 1;
                }
                let mut e = j;
                while e < b.len() && !matches!(b[e], b' ' | b'"' | b'\'' | b'\n' | b'\r' | b',' | b';' | b')' | b'}') {
                    e += 1;
                }
                let v = &line[j..e];
                if v.len() >= 8 && !v.starts_with("[REDACTED") && !v.starts_with('$') && !v.starts_with("os.") && !v.starts_with("process.") {
                    let m = put_secret(v, None, &mut map);
                    named_out.push_str(&line[cursor..j]);
                    named_out.push_str(&m);
                    cursor = e;
                    replaced = true;
                }
            }
            if !replaced {
                named_out.push_str(&line[cursor..i]);
                cursor = i;
            }
        }
    }
    // "Bearer <token>" forms: scan every match on each line.
    let mut res = String::with_capacity(named_out.len());
    for line in named_out.split_inclusive('\n') {
        let mut cursor = 0;
        while let Some(rel) = line[cursor..].find("Bearer ") {
            let s = cursor + rel + 7;
            let e = line[s..].find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';')).map_or(line.len(), |x| s + x);
            res.push_str(&line[cursor..s]);
            if e - s >= 12 && !line[s..e].starts_with("[REDACTED") {
                res.push_str(&put_secret(&line[s..e], None, &mut map));
                cursor = e;
            } else {
                cursor = s;
            }
        }
        res.push_str(&line[cursor..]);
    }
    let res = url_credentials(&res, &mut map);
    let out = pii(&res, &mut map);
    (out, map)
}

/// Restores `[REDACTED_*]` placeholders using a map returned by `redact`.
/// Single-pass and longest-mark-first so restored values cannot trigger recursive expansion or
/// collide between `[REDACTED_1]` and `[REDACTED_10]`.
pub fn unredact(text: &str, map: &[V]) -> (String, usize) {
    let mut pairs: Vec<(&str, &str)> = map
        .iter()
        .filter_map(|m| {
            let mark = m.get("mark").s();
            if mark.is_empty() { None } else { Some((mark, m.get("value").s())) }
        })
        .collect();
    if pairs.is_empty() || text.is_empty() {
        return (text.to_string(), 0);
    }
    pairs.sort_by_key(|p| core::cmp::Reverse(p.0.len()));
    let mut out = String::with_capacity(text.len());
    let mut restored = 0usize;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let mut hit = None;
        for &(mark, val) in &pairs {
            if b[i] == mark.as_bytes()[0] && text[i..].starts_with(mark) {
                hit = Some((mark.len(), val));
                break;
            }
        }
        if let Some((mlen, val)) = hit {
            out.push_str(val);
            restored += 1;
            i += mlen;
        } else {
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    (out, restored)
}

const MAX_EXPAND_BYTES: usize = 16 * 1024 * 1024;
/// References replaced in one call. An archive whose originals reference each other many times
/// over could otherwise make a tiny input do an exponential amount of work.
const MAX_EXPANSIONS: usize = 100_000;
/// A reference marker is one short line. Bounding the search keeps text full of `[` linear.
const MAX_MARKER_BYTES: usize = 4096;

pub struct ExpandOut {
    pub text: String,
    pub expanded: Vec<String>,
    pub missing: Vec<String>,
    pub cycles: usize,
    pub truncated: bool,
}

fn is_ref_id(s: &str) -> bool {
    (6..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

struct Marker<'a> {
    id: &'a str,
    len: usize,
    /// For `[#id log N lines, M distinct]`: M, the most sample lines `compress` kept after it.
    log_samples: Option<usize>,
}

/// Parses a `[#id ...]` or `[same as #id]` reference at the start of `s`.
fn parse_marker(s: &str) -> Option<Marker<'_>> {
    if !s.starts_with('[') {
        return None;
    }
    let close = s.bytes().take(MAX_MARKER_BYTES).position(|c| c == b']' || c == b'\n' || c == b'\r')?;
    if s.as_bytes()[close] != b']' {
        return None;
    }
    let inner = &s[1..close];
    if let Some(rest) = inner.strip_prefix('#') {
        let mut words = rest.split_whitespace();
        let id = words.next()?;
        if !is_ref_id(id) {
            return None;
        }
        let log_samples = if words.next() == Some("log") {
            let w: Vec<&str> = rest.split_whitespace().collect();
            w.iter().position(|x| *x == "distinct").and_then(|p| p.checked_sub(1)).and_then(|p| w[p].parse().ok())
        } else {
            None
        };
        return Some(Marker { id, len: close + 1, log_samples });
    }
    let id = inner.strip_prefix("same as #")?.trim();
    is_ref_id(id).then_some(Marker { id, len: close + 1, log_samples: None })
}

/// Bytes of the sample lines that `compress` kept after a log reference: lines of the original
/// run, each optionally followed by ` (xN)`. Expanding the reference makes them redundant.
fn log_sample_len(rest: &str, body: &str, max_lines: usize) -> usize {
    if !rest.starts_with('\n') {
        return 0;
    }
    let lines: BTreeSet<&str> = body.split('\n').collect();
    let mut skip = 0;
    for _ in 0..max_lines {
        let Some(tail) = rest[skip..].strip_prefix('\n') else { break };
        let line = tail.split('\n').next().unwrap_or("");
        let base = match line.rfind(" (x") {
            Some(p) if line.ends_with(')') && line[p + 3..line.len() - 1].bytes().all(|c| c.is_ascii_digit()) && line.len() > p + 4 => &line[..p],
            _ => line,
        };
        if line.is_empty() || !(lines.contains(base) || lines.contains(line)) {
            break;
        }
        skip += 1 + line.len();
    }
    skip
}

struct Expander<'a> {
    archive: &'a V,
    only: &'a [String],
    limit_depth: usize,
    visiting: Vec<String>,
    expanded: Vec<String>,
    missing: Vec<String>,
    cycles: usize,
    budget: usize,
    truncated: bool,
    out: String,
}

impl<'a> Expander<'a> {
    fn lookup(&self, id: &str) -> Option<&'a str> {
        let archive: &'a V = self.archive;
        match archive.get(id) {
            V::Str(s) => Some(s.as_str()),
            _ => match archive.get(&format!("#{id}")) {
                V::Str(s) => Some(s.as_str()),
                _ => None,
            },
        }
    }

    fn push_capped(&mut self, s: &str) {
        let room = MAX_EXPAND_BYTES.saturating_sub(self.out.len());
        if s.len() <= room {
            self.out.push_str(s);
        } else {
            let mut cut = room;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            self.out.push_str(&s[..cut]);
            self.truncated = true;
        }
    }

    fn walk(&mut self, src: &str, depth: usize) {
        let mut i = 0;
        let mut plain = 0;
        while i < src.len() {
            if self.truncated {
                return;
            }
            let Some(rel) = src[i..].find('[') else { break };
            i += rel;
            let Some(m) = parse_marker(&src[i..]) else {
                i += 1;
                continue;
            };
            let allowed = self.only.is_empty() || self.only.iter().any(|x| x.trim_start_matches('#') == m.id);
            if !allowed {
                i += m.len;
                continue;
            }
            let Some(body) = self.lookup(m.id) else {
                if !self.missing.iter().any(|x| x == m.id) {
                    self.missing.push(m.id.to_string());
                }
                i += m.len;
                continue;
            };
            if self.visiting.iter().any(|v| v == m.id) {
                self.cycles += 1;
                i += m.len;
                continue;
            }
            if self.budget == 0 {
                self.truncated = true;
                i += m.len;
                continue;
            }
            self.budget -= 1;
            self.push_capped(&src[plain..i]);
            if !self.expanded.iter().any(|x| x == m.id) {
                self.expanded.push(m.id.to_string());
            }
            if depth + 1 < self.limit_depth {
                self.visiting.push(m.id.to_string());
                self.walk(body, depth + 1);
                self.visiting.pop();
            } else {
                self.push_capped(body);
            }
            i += m.len;
            if let Some(n) = m.log_samples {
                i += log_sample_len(&src[i..], body, n);
            }
            plain = i;
        }
        if !self.truncated {
            self.push_capped(&src[plain..]);
        }
    }
}

/// Rehydrates `[#id ...]` and `[same as #id]` references against an archive object (`id -> body`).
/// Originals that contain references are expanded too, up to `max_depth` levels. A reference to an
/// id already being expanded is a cycle and stays as it is. For a log reference, the sample lines
/// that `compress` kept after it are dropped, as the original contains them.
pub fn expand(text: &str, archive: &V, only: &[String], max_depth: usize) -> ExpandOut {
    let mut x = Expander {
        archive,
        only,
        limit_depth: max_depth.clamp(1, 8),
        visiting: Vec::new(),
        expanded: Vec::new(),
        missing: Vec::new(),
        cycles: 0,
        budget: MAX_EXPANSIONS,
        truncated: false,
        out: String::with_capacity(text.len()),
    };
    x.walk(text, 0);
    ExpandOut { text: x.out, expanded: x.expanded, missing: x.missing, cycles: x.cycles, truncated: x.truncated }
}

/// Every distinct archive id referenced as `[#id ...]` or `[same as #id]`, in order of appearance.
pub fn ref_ids(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find('[') {
        i += rel;
        match parse_marker(&text[i..]) {
            Some(m) => {
                if !ids.iter().any(|x| x == m.id) {
                    ids.push(m.id.to_string());
                }
                i += m.len;
            }
            None => i += 1,
        }
    }
    ids
}

/// `scheme://user:password@host`: the password goes, the rest of the address stays readable.
fn url_credentials(t: &str, map: &mut Vec<V>) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '/' | '"' | '\'' | '`' | '>' | ')')).unwrap_or(tail.len());
        let auth = &tail[..end];
        if let Some(at) = auth.rfind('@') {
            if let Some(colon) = auth[..at].find(':') {
                let secret = &auth[colon + 1..at];
                if !secret.is_empty() && !secret.starts_with("[REDACTED") {
                    let mark = put_secret(secret, None, map);
                    out.push_str(&auth[..colon + 1]);
                    out.push_str(&mark);
                    out.push_str(&tail[at..end]);
                    rest = &tail[end..];
                    continue;
                }
            }
        }
        out.push_str(auth);
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
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
                    o.push_str(&put_secret(&t[i..end], Some("EMAIL"), map));
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
                    o.push_str(&put_secret(&t[i..e], Some(kind), map));
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
