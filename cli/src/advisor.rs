//! A stronger model on call. It reads a capped brief, never the transcript, and only when the gate opens.
use crate::backends::{self, Reply};
use crate::engine::Engine;
use crate::ledger;
use crate::util::{find_in_path, k, log, run_process, strs, R};
use ctxflow_kernel::json::{obj, st, V};
use std::time::Duration;

const RESTORE_CAP_CHARS: usize = 6000;

const SCOUT_SYSTEM: &str = "Look through the repository to answer the question, then report what you found.
You can only read. Reply with at most 15 lines. Give file:line references instead of pasting code.
State what you did not find. No preamble.";

pub struct Advice {
    pub consulted: bool,
    pub reason: String,
    pub text: String,
    pub tokens: u64,
    pub dropped: Vec<String>,
    pub restored: Vec<String>,
}

fn working_diff(eng: &Engine) -> String {
    // Git runs programs named in a repository's own config (fsmonitor, pager, external diff,
    // textconv). Only trusted projects get a diff, and even then those hooks are switched off.
    if !eng.cfg.get("trusted").b() {
        return String::new();
    }
    let Some(git) = find_in_path("git") else { return String::new() };
    let args: Vec<String> = [
        "-c", "core.fsmonitor=false", "-c", "core.pager=cat", "-c", "diff.external=", "--no-pager",
        "diff", "--no-ext-diff", "--no-textconv", "HEAD", "--stat", "-p",
    ].iter().map(|s| s.to_string()).collect();
    match run_process(&git, &args, "", Some(&eng.p.root), Duration::from_secs(20)) {
        Ok(o) if o.ok => o.stdout,
        _ => String::new(),
    }
}

/// The ids in a `NEED: #id #id` line, at most two.
fn needed_ids(reply: &str) -> Vec<String> {
    let Some(line) = reply.lines().find_map(|l| l.trim().strip_prefix("NEED:")) else { return Vec::new() };
    line.split(|c: char| !c.is_ascii_hexdigit()).filter(|w| w.len() == 12).take(2).map(str::to_string).collect()
}

pub fn advise(eng: &Engine, trigger: &str, question: &str, error: &str) -> R<Advice> {
    let mut s = eng.p.load_session();
    let a = eng.cfg.get("advisor");
    let diff = if trigger == "done" { working_diff(eng) } else { String::new() };
    // Review once per state of the code, not once per turn that ends with uncommitted work.
    let diff_id = if diff.is_empty() { String::new() } else { k("error_sig", vec![("text", st(&diff))]).get("sig").s().to_string() };
    let changed = !diff.is_empty() && diff_id != s.advisor.get("reviewed_diff").s();
    let sig = if trigger == "error" {
        let source = if error.is_empty() { question } else { error };
        k("error_sig", vec![("text", st(source))]).get("sig").s().to_string()
    } else {
        String::new()
    };
    let g = k("gate", vec![
        ("trigger", st(trigger)),
        ("state", s.advisor.clone()),
        ("error_sig", st(&sig)),
        ("has_changes", V::Bool(changed)),
        ("limits", obj(vec![
            ("max_calls", a.get("max_calls").clone()),
            ("max_tokens", a.get("max_tokens").clone()),
            ("error_repeat", a.get("error_repeat").clone()),
        ])),
    ]);
    s.advisor = g.get("state").clone();
    let reason = g.get("reason").s().to_string();
    if !g.get("consult").b() {
        eng.p.save_session(&s)?;
        return Ok(Advice { consulted: false, reason, text: String::new(), tokens: 0, dropped: Vec::new(), restored: Vec::new() });
    }
    if trigger == "done" {
        s.advisor.set("reviewed_diff", st(&diff_id));
    }

    let recent: Vec<String> = s.turns.iter().rev().take(2).rev().map(|t| format!("[user] {}\n[assistant] {}", t.user, t.assistant)).collect();
    let section = |name: &str, text: &str| obj(vec![("name", st(name)), ("text", st(text))]);
    let mut sections = vec![section("QUESTION", &format!("Checkpoint: {reason}\n{question}"))];
    if !error.is_empty() {
        sections.push(section("ERROR", error));
    }
    sections.extend([
        section("GOAL", &eng.p.read_doc("SPEC.md")),
        section("CHANGES", &diff),
        section("PROGRESS", &eng.p.read_doc("PROGRESS.md")),
        section("SUMMARY", &s.summary),
        section("RECENT", &recent.join("\n\n")),
    ]);
    let b = k("brief", vec![("cap", a.get("brief_tokens").clone()), ("sections", V::Arr(sections))]);
    eng.p.archive_put(b.get("stubs").arr())?;
    let system = k("advisor_system", vec![]).get("text").s().to_string();
    let brief = b.get("brief").s().to_string();
    let kind = format!("advisor/{trigger}");
    let mut res = eng.route("advisor", &system, &brief, &kind, None, Some(300))?;
    let mut tokens = res.total_tokens;
    let mut restored = Vec::new();

    // One pull for what the brief left out. This is the only way it reads more.
    let mut parts = Vec::new();
    for id in needed_ids(&res.text) {
        if let Some(body) = eng.p.archive_get(&id) {
            let capped: String = body.chars().take(RESTORE_CAP_CHARS).collect();
            parts.push(format!("=== #{id} ===\n{capped}"));
            restored.push(id);
        }
    }
    if !parts.is_empty() {
        let again = format!("{system}\nThe references you asked for are included. Decide now.");
        res = eng.route("advisor", &again, &format!("{brief}\n\n{}", parts.join("\n\n")), &kind, None, Some(300))?;
        tokens += res.total_tokens;
    }
    s.advisor.set("tokens", V::Num(s.advisor.get("tokens").n(0.0) + tokens as f64));
    s.spent_tokens += tokens;
    eng.p.save_session(&s)?;
    Ok(Advice { consulted: true, reason, text: res.text.trim().to_string(), tokens, dropped: strs(b.get("dropped")), restored })
}

/// A cheap read-only search that hands back a summary, so the search output never enters the context.
pub fn scout(eng: &Engine, question: &str) -> R<Reply> {
    let Some(model) = strs(eng.cfg.get("routing").get("scout")).into_iter().find(|c| c.starts_with("claude:")) else {
        return Err("scout needs a claude: model in routing.scout, because it needs file tools.".to_string());
    };
    let mut cfg = eng.cfg.clone();
    let mut claude = cfg.get("claude").clone();
    let args = ["--tools", "Read,Grep,Glob", "--no-session-persistence", "--disable-slash-commands", "--strict-mcp-config", "--setting-sources", "user"];
    claude.set("extra_args", V::Arr(args.iter().map(|x| st(x)).collect()));
    claude.set("cwd", st(&eng.p.root.display().to_string()));
    cfg.set("claude", claude);
    log(&format!("scout → {model} (read-only)"));
    let r = backends::call(&model, SCOUT_SYSTEM, question, &cfg, None).map_err(|e| e.msg)?;
    ledger::record(&model, r.input_tokens, r.output_tokens, "scout", r.estimated, &eng.p.root.display().to_string(), r.cost_usd);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn need_lines_yield_at_most_two_ids() {
        assert_eq!(needed_ids("NEED: #0123456789ab"), vec!["0123456789ab"]);
        assert_eq!(needed_ids("x\n  NEED: #aaaaaaaaaaaa, bbbbbbbbbbbb #cccccccccccc"), vec!["aaaaaaaaaaaa", "bbbbbbbbbbbb"]);
        assert!(needed_ids("OK: fine, see #0123456789ab").is_empty());
        assert!(needed_ids("NEED: #short").is_empty());
    }
}
