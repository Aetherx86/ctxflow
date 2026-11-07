//! ctxflow: a token-frugal LLM workflow. One binary, with the decision kernel linked in.
mod advisor;
mod backends;
mod config;
mod council;
mod engine;
mod http;
mod ledger;
mod serve;
mod state;
mod util;

use crate::engine::{AskOptions, Engine};
use crate::state::{Project, Session};
use crate::util::{dim, say, strs, R};
use ctxflow_kernel::compress::estimate;
use ctxflow_kernel::json::V;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const HELP: &str = "ctxflow: token-frugal LLM workflow with reversible compression, a gated advisor and a council.

Usage: ctxflow [-C DIR] [--profile NAME] COMMAND [ARGS]

Start
  init [--no-docs]            create .ctxflow/config.toml, SPEC.md, PROGRESS.md
  profile [list|set NAME|clear]  choose which providers to use
  mode [fast|auto|thorough]   fast: answers only. auto: the gate decides. thorough: full council
  serve [--port N] [--no-open]  open the app, with this machine's keys and models

Ask
  ask [MSG] [-f FILE] [-r ID] [-t light|heavy|summary] [-m MODEL] [--force] [--no-remember] [-q]
  chat [--force]              interactive session
  prompt [MSG] [-f FILE] [--force]  print the assembled prompt without calling a model

Decide
  council [MSG] [-o OPTION]... [-y] [-v] [--json]  independent answers, anonymous scoring, one decision
  advise plan|error|done|manual [MSG] [--stdin]    ask the stronger model, if the gate allows it
  scout [MSG]                 cheap read-only repository search; returns a summary

Keep state
  show ID                     print a stored original by id
  status                      session size and token usage against budgets
  line                        one status line (no model calls)
  fold                        roll all kept turns into the summary now
  progress [NOTE]             merge pending PROGRESS DELTAs into PROGRESS.md
  handoff [NOTE]              summary, PROGRESS.md, HANDOFF.md, fresh session
  new [--blank]               start a new session (keeps the summary unless --blank)

A message can also come from stdin. Models are written backend:model, such as ollama:auto,
gemini:gemini-3.5-flash, claude:haiku, anthropic:claude-opus-5-5 or openrouter:vendor/model.";

// ── arguments ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Parsed {
    pos: Vec<String>,
    values: BTreeMap<&'static str, Vec<String>>,
    flags: BTreeSet<&'static str>,
}

impl Parsed {
    fn one(&self, name: &str) -> Option<&str> {
        self.values.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    fn many(&self, name: &str) -> &[String] {
        self.values.get(name).map_or(&[], Vec::as_slice)
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains(name)
    }
}

/// `spec` holds (short, long, takes a value).
fn parse_args(args: &[String], spec: &[(&'static str, &'static str, bool)]) -> R<Parsed> {
    let mut p = Parsed::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--" {
            p.pos.extend(it.by_ref().cloned());
            break;
        }
        if arg.len() < 2 || !arg.starts_with('-') {
            p.pos.push(arg.clone());
            continue;
        }
        let (name, inline) = arg.split_once('=').map_or((arg.as_str(), None), |(n, v)| (n, Some(v.to_string())));
        let Some(&(_, long, takes)) = spec.iter().find(|(s, l, _)| name == *s || name == *l) else {
            return Err(format!("unknown option {name}. Run `ctxflow --help`. To send text that starts with a dash, put it after `--`."));
        };
        if takes {
            let Some(value) = inline.or_else(|| it.next().cloned()) else {
                return Err(format!("{name} needs a value"));
            };
            p.values.entry(long).or_default().push(value);
        } else {
            p.flags.insert(long);
        }
    }
    Ok(p)
}

fn read_stdin_all() -> String {
    let mut text = String::new();
    let mut bytes = Vec::new();
    if std::io::stdin().read_to_end(&mut bytes).is_ok() {
        text = String::from_utf8_lossy(&bytes).trim().to_string();
    }
    text
}

fn read_message(parts: &[String]) -> R<String> {
    let mut msg = parts.join(" ").trim().to_string();
    if msg.is_empty() && !std::io::stdin().is_terminal() {
        msg = read_stdin_all();
    }
    if msg.is_empty() {
        return Err("empty message".to_string());
    }
    Ok(msg)
}

/// Reads stdin only if data arrives quickly. Hooks often leave it open and silent, and that must
/// not hang. `--stdin` waits for the end of input, for commands that take time to produce it.
fn piped_input(wait: bool) -> String {
    if std::io::stdin().is_terminal() {
        return String::new();
    }
    if wait {
        return read_stdin_all();
    }
    let (tx, rx) = mpsc::channel::<Option<Vec<u8>>>();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 16384];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
                Ok(n) => {
                    if tx.send(Some(chunk[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let mut bytes = match rx.recv_timeout(Duration::from_millis(200)) {
        Ok(Some(first)) => first,
        _ => return String::new(),
    };
    while let Ok(Some(more)) = rx.recv() {
        bytes.extend_from_slice(&more);
    }
    String::from_utf8_lossy(&bytes).trim().to_string()
}

struct Global {
    dir: Option<String>,
    profile: Option<String>,
}

impl Global {
    fn start(&self) -> PathBuf {
        PathBuf::from(self.dir.as_deref().unwrap_or("."))
    }

    fn project(&self) -> Project {
        Project::find(&self.start())
    }

    fn engine(&self, require_init: bool) -> R<Engine> {
        let p = self.project();
        if require_init && !p.initialized() {
            return Err("not initialized here. Run `ctxflow init` in the project root.".to_string());
        }
        let cfg = config::load(&p.root, self.profile.as_deref())?;
        Ok(Engine { p, cfg })
    }

    fn initialized(&self) -> R<Project> {
        let p = self.project();
        if !p.initialized() {
            return Err("not initialized here. Run `ctxflow init` first.".to_string());
        }
        Ok(p)
    }
}

// ── commands ─────────────────────────────────────────────────────────────────────

fn print_answer(ans: &engine::Answer, quiet: bool) {
    say(&ans.text);
    if !quiet {
        let r = &ans.reply;
        let flag = if r.estimated { "~" } else { "" };
        dim(&format!("\n── {} · task={} · in {flag}{} / out {flag}{} tokens", r.model, ans.task, r.input_tokens, r.output_tokens));
    }
}

fn cmd_init(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("", "--no-docs", false)])?;
    let found = g.project();
    let proj = if found.initialized() {
        found
    } else {
        let start = g.start();
        Project::at(start.canonicalize().unwrap_or(start))
    };
    let created = proj.init(!a.has("--no-docs"))?;
    say(&format!("ctxflow initialized at {}", proj.root.display()));
    for c in created {
        say(&format!("  + {c}"));
    }
    Ok(())
}

const ASK_FLAGS: &[(&str, &str, bool)] = &[
    ("-f", "--file", true),
    ("", "--force", false),
    ("-t", "--task", true),
    ("-m", "--model", true),
    ("", "--no-remember", false),
    ("-r", "--ref", true),
    ("-q", "--quiet", false),
];

fn cmd_ask(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, ASK_FLAGS)?;
    let task = a.one("--task");
    if let Some(t) = task {
        if !["light", "heavy", "summary"].contains(&t) {
            return Err(format!("--task must be light, heavy or summary, not {t:?}"));
        }
    }
    let eng = g.engine(true)?;
    let ans = eng.ask(&read_message(&a.pos)?, &AskOptions {
        files: a.many("--file"),
        task,
        model: a.one("--model"),
        force: a.has("--force"),
        remember: !a.has("--no-remember"),
        refs: a.many("--ref"),
    })?;
    print_answer(&ans, a.has("--quiet"));
    Ok(())
}

fn cmd_prompt(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("-f", "--file", true), ("", "--force", false)])?;
    let eng = g.engine(true)?;
    let s = eng.p.load_session();
    let msg = read_message(&a.pos)?;
    let prompt = eng.build_prompt(&s, &msg, a.many("--file"), a.has("--force"), &[])?;
    let tokens = estimate(&prompt);
    say(&prompt);
    dim(&format!("\n── ~{tokens} tokens · would route as task={}", eng.classify(&msg, tokens)));
    Ok(())
}

fn print_status(eng: &Engine) {
    let init = eng.p.initialized();
    let s = if init { eng.p.load_session() } else { Session::new("") };
    say(&format!("project : {}{}", eng.p.root.display(), if init { "" } else { "  (not initialized)" }));
    say(&format!(
        "session : {} · {} turn(s) kept · summary {} tok · history ~{} tok · spent {} tok · {} pending delta(s)",
        s.id, s.turns.len(), estimate(&s.summary), eng.history_tokens(&s), s.spent_tokens, s.pending_progress.len()
    ));
    for name in strs(eng.cfg.get("context").get("state_files")) {
        say(&format!("  {name:<12} ~{} tok", estimate(&eng.p.read_doc(&name))));
    }
    let hint = eng.cfg.get("context").get("handoff_hint_tokens").n(60000.0) as u64;
    if s.spent_tokens >= hint {
        say(&format!("  spent >= {hint}: run `ctxflow handoff` to start a fresh, cheap session"));
    }
    say(&format!("mode    : {} · profile: {}", eng.cfg.get("mode").s(), match eng.cfg.get("active_profile").s() { "" => "none", p => p }));
    say("\nusage today (since 00:00 Pacific) / last 60s, vs. configured limits:");
    let stats = ledger::usage_window();
    let specs: BTreeSet<String> = stats.keys().cloned().chain(engine::routed_models(&eng.cfg)).collect();
    for spec in specs {
        let u = stats.get(&spec).copied().unwrap_or_default();
        let lim = config::model_budget(&eng.cfg, &spec);
        let cells: Vec<String> = ["rpd", "tpd", "rpm", "tpm"].iter().map(|key| {
            let limit = lim.get(key).n(0.0) as u64;
            if limit > 0 { format!("{key}={}/{limit}", u.field(key)) } else { format!("{key}={}", u.field(key)) }
        }).collect();
        say(&format!("  {spec:<34} {}", cells.join("  ")));
    }
}

fn cmd_status(g: &Global) -> R<()> {
    print_status(&g.engine(false)?);
    Ok(())
}

fn cmd_show(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let Some(id) = a.pos.first() else { return Err("show needs an id".to_string()) };
    let Some(body) = g.engine(true)?.p.archive_get(id) else {
        return Err(format!("nothing stored under {id}"));
    };
    // Exact bytes: this is the promise of the archive, so no newline is added.
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(body.as_bytes());
    let _ = out.flush();
    Ok(())
}

struct CouncilFlags<'a> {
    options: &'a [String],
    yes: bool,
    json: bool,
    verbose: bool,
    auto: bool,
}

fn run_council(eng: &Engine, question: &str, f: &CouncilFlags) -> R<()> {
    let seats = config::council_seats(&eng.cfg).0;
    let pre = council::preflight(eng, question, seats.len());
    let c = eng.cfg.get("council");
    let today = if pre.daily > 0 { format!(" · {} left today", pre.left_today) } else { String::new() };
    dim(&format!("── at most ~{} tokens · limit {}{today}", pre.worst_case, c.get("max_tokens").n(0.0) as u64));
    if pre.worst_case > c.get("confirm_over").n(0.0) as u64 && !f.yes {
        if f.auto {
            dim("── above [council] confirm_over, so it was not started automatically");
            return Ok(());
        }
        if !std::io::stdin().is_terminal() {
            return Err("above [council] confirm_over. Pass --yes to run.".to_string());
        }
        eprint!("Run it? [y/N] ");
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        if line.trim().to_lowercase() != "y" {
            return Ok(());
        }
    }
    let (json, verbose) = (f.json, f.verbose);
    let mut on_event = |ev: &V| match ev.get("type").s() {
        "stage" if !json => dim(&format!("── {}", ev.get("title").s())),
        "seat_error" => dim(&format!("   seat {} failed: {}", ev.get("seat").n(0.0), ev.get("error").s())),
        "seat_end" if verbose => say(&format!("\n### {}\n{}", ev.get("model").s(), ev.get("opinion").s())),
        _ => {}
    };
    let result = council::run(eng, question, f.options, &mut on_event)?;
    let (used_in, used_out) = (result.get("used").get("in").n(0.0), result.get("used").get("out").n(0.0));
    let (mode, final_text) = (result.get("mode").s(), result.get("final").s());
    if f.json {
        say(&result.dump());
    } else {
        say(final_text);
        if !result.get("matrix").arr().is_empty() {
            say("\n| Model | Evidence | Risks | Feasible | Rank score | Flaws |\n|---|---|---|---|---|---|");
            for r in result.get("matrix").arr() {
                let cell = |key: &str, digits: usize| match r.get(key) {
                    V::Num(n) => format!("{n:.digits$}"),
                    _ => "-".to_string(),
                };
                say(&format!("| {} | {} | {} | {} | {} | {} |", r.get("model").s(), cell("evidence", 1), cell("risk", 1), cell("feasibility", 1), cell("borda", 2), r.get("fatal").arr().len()));
            }
        }
        let removed = result.get("redacted").n(0.0);
        dim(&format!(
            "\n── {mode} · {} distinct model(s) · {} calls · in {} / out {} tokens{}",
            result.get("independent_models").n(0.0), result.get("calls").n(0.0), used_in, used_out,
            if removed > 0.0 { format!(" · {removed} secret(s) removed") } else { String::new() }
        ));
        if result.get("independent_models").n(0.0) < 2.0 && mode != "failed" {
            dim("── one model only, so this is not independent agreement");
        }
    }
    let mut s = eng.p.load_session();
    s.spent_tokens += (used_in + used_out) as u64;
    let short = |t: &str, n: usize| t.chars().take(n).collect::<String>();
    s.pending_progress.push(format!("- council ({mode}): {} -> {}", short(question, 80), short(final_text, 160)));
    eng.p.save_session(&s)
}

fn cmd_council(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("-o", "--option", true), ("-y", "--yes", false), ("-v", "--verbose", false), ("", "--json", false)])?;
    let eng = g.engine(true)?;
    let question = read_message(&a.pos)?;
    run_council(&eng, &question, &CouncilFlags { options: a.many("--option"), yes: a.has("--yes"), json: a.has("--json"), verbose: a.has("--verbose"), auto: false })
}

fn cmd_advise(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("", "--stdin", false)])?;
    let Some((trigger, rest)) = a.pos.split_first() else {
        return Err("advise needs a checkpoint: plan, error, done or manual".to_string());
    };
    if !["plan", "error", "done", "manual"].contains(&trigger.as_str()) {
        return Err(format!("advise takes plan, error, done or manual, not {trigger:?}"));
    }
    let eng = g.engine(true)?;
    let mut question = rest.join(" ").trim().to_string();
    let mut error = piped_input(a.has("--stdin"));
    if trigger != "error" && !error.is_empty() && question.is_empty() {
        question = std::mem::take(&mut error);
    }
    if question.is_empty() {
        question = "Review the current state.".to_string();
    }
    let mode = eng.cfg.get("mode").s();
    let auto = CouncilFlags { options: &[], yes: false, json: false, verbose: false, auto: true };
    // Fast: only an explicit request reaches the stronger model.
    if mode == "fast" && trigger != "manual" {
        dim("── silent: Fast mode");
        return Ok(());
    }
    // Thorough: checkpoints that shape the work get the full council instead of one opinion.
    if mode == "thorough" && (trigger == "plan" || trigger == "done") {
        return run_council(&eng, &question, &auto);
    }
    let adv = advisor::advise(&eng, trigger, &question, &error)?;
    if !adv.consulted {
        dim(&format!("── {}", adv.reason));
        return Ok(());
    }
    say(&adv.text);
    let used = eng.p.load_session().advisor.get("calls").n(0.0);
    let mut tail = String::new();
    if !adv.dropped.is_empty() {
        tail += &format!(" · left out: {}", adv.dropped.join(", "));
    }
    if !adv.restored.is_empty() {
        tail += &format!(" · restored: {}", adv.restored.join(", "));
    }
    dim(&format!("\n── {} · call {used}/{} · {} tokens{tail}", adv.reason, eng.cfg.get("advisor").get("max_calls").n(0.0), adv.tokens));
    if adv.text.starts_with("ESCALATE") {
        if mode == "fast" {
            dim("── Fast mode: no council. Run `ctxflow council` to decide this");
        } else {
            let asked = adv.text.split_once(':').map_or("", |(_, q)| q.trim());
            run_council(&eng, if asked.is_empty() { &question } else { asked }, &auto)?;
        }
    }
    Ok(())
}

fn cmd_scout(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let eng = g.engine(true)?;
    let r = advisor::scout(&eng, &read_message(&a.pos)?)?;
    say(&r.text);
    dim(&format!("── {} · in {} / out {} tokens", r.model, r.input_tokens, r.output_tokens));
    Ok(())
}

fn cmd_fold(g: &Global) -> R<()> {
    let s = g.engine(true)?.fold_now()?;
    say(if s.summary.is_empty() { "(nothing to fold)" } else { &s.summary });
    Ok(())
}

fn cmd_progress(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let eng = g.engine(true)?;
    let mut s = eng.p.load_session();
    let out = eng.update_progress(&mut s, &a.pos.join(" "))?;
    eng.p.save_session(&s)?;
    say(out.as_deref().unwrap_or("(no pending deltas — PROGRESS.md unchanged)"));
    Ok(())
}

fn cmd_handoff(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let eng = g.engine(true)?;
    let (text, archived) = eng.handoff(&a.pos.join(" "))?;
    say(&text);
    let shown = archived.strip_prefix(&eng.p.root).unwrap_or(&archived).display().to_string();
    dim(&format!("── archived old session → {shown}; new session starts from the handoff summary"));
    Ok(())
}

fn cmd_new(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("", "--blank", false)])?;
    let eng = g.engine(true)?;
    let old = eng.p.load_session();
    if !old.turns.is_empty() || !old.summary.is_empty() {
        eng.p.archive_session(&old)?;
    }
    let blank = a.has("--blank");
    eng.p.save_session(&Session::new(if blank { "" } else { &old.summary }))?;
    say(if blank { "new session started" } else { "new session started (rolling summary carried over)" });
    Ok(())
}

fn cmd_chat(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("", "--force", false)])?;
    let eng = g.engine(true)?;
    say("ctxflow chat — /file <path>, /task light|heavy|auto, /model <spec|auto>, /fold, /progress, /handoff, /status, /quit");
    let (mut files, mut task, mut model): (Vec<String>, Option<String>, Option<String>) = (Vec::new(), None, None);
    let stdin = std::io::stdin();
    loop {
        print!("\n› ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            say("");
            return Ok(());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(command) = line.strip_prefix('/') {
            let (cmd, rest) = command.split_once(' ').map_or((command, ""), |(c, r)| (c, r.trim()));
            let auto = |v: &str| if v.is_empty() || v == "auto" { None } else { Some(v.to_string()) };
            let outcome: R<()> = (|| {
                match cmd {
                    "q" | "quit" | "exit" => std::process::exit(0),
                    "file" => {
                        files = if rest.is_empty() { Vec::new() } else { vec![rest.to_string()] };
                        say(&format!("attached for next message: {}", if files.is_empty() { "none" } else { rest }));
                    }
                    "task" => {
                        task = auto(rest);
                        say(&format!("task = {}", task.as_deref().unwrap_or("auto")));
                    }
                    "model" => {
                        model = auto(rest);
                        say(&format!("model = {}", model.as_deref().unwrap_or("auto (routing)")));
                    }
                    "fold" => {
                        let s = eng.fold_now()?;
                        say(if s.summary.is_empty() { "(nothing to fold)" } else { &s.summary });
                    }
                    "progress" => {
                        let mut s = eng.p.load_session();
                        let out = eng.update_progress(&mut s, "")?;
                        eng.p.save_session(&s)?;
                        say(out.as_deref().unwrap_or("(no pending deltas)"));
                    }
                    "handoff" => say(&eng.handoff(rest)?.0),
                    "status" => print_status(&eng),
                    other => say(&format!("unknown command /{other}")),
                }
                Ok(())
            })();
            if let Err(e) = outcome {
                eprintln!("ctxflow: {e}");
            }
            continue;
        }
        let options = AskOptions { files: &files, task: task.as_deref(), model: model.as_deref(), force: a.has("--force"), remember: true, refs: &[] };
        match eng.ask(line, &options) {
            Ok(ans) => {
                files.clear(); // attachments are one-shot: never re-sent on later turns
                print_answer(&ans, false);
            }
            Err(e) => eprintln!("ctxflow: {e}"),
        }
    }
}

fn cmd_profile(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let proj = g.initialized()?;
    let cfg = config::load(&proj.root, None)?;
    let marker = proj.dir.join("profile");
    match a.pos.first().map(String::as_str) {
        Some("set") => {
            let Some(name) = a.pos.get(1) else { return Err("profile set needs a name".to_string()) };
            config::load(&proj.root, Some(name))?;
            std::fs::write(&marker, format!("{name}\n")).map_err(|e| e.to_string())?;
            say(&format!("profile = {name}"));
        }
        Some("clear") => {
            let _ = std::fs::remove_file(&marker);
            say("profile cleared; using [routing] from config.toml");
        }
        None | Some("list") => {
            let d = serve::detect(&cfg);
            let V::Obj(found) = &d else { return Ok(()) };
            let shown: Vec<String> = found.iter().map(|(k, v)| format!("{k} {}", if v.b() { "yes" } else { "no" })).collect();
            say(&format!("detected: {}", shown.join(", ")));
            let V::Obj(all) = cfg.get("profiles") else { return Ok(()) };
            for (name, p) in all {
                let ready = match name.as_str() {
                    "local" => d.get("local").b(),
                    "free" => d.get("gemini").b(),
                    "balanced" => d.get("gemini").b() && d.get("claude").b(),
                    "claude" => d.get("claude").b(),
                    "api" => d.get("anthropic").b() || d.get("openrouter").b(),
                    _ => true,
                };
                let mark = if name == cfg.get("active_profile").s() { "*" } else { " " };
                say(&format!("{mark} {name:<10} {} {}", if ready { "ready " } else { "      " }, p.get("about").s()));
            }
        }
        Some(other) => return Err(format!("profile takes list, set NAME or clear, not {other:?}")),
    }
    Ok(())
}

fn cmd_mode(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[])?;
    let proj = g.initialized()?;
    if let Some(v) = a.pos.first() {
        if !["fast", "auto", "thorough"].contains(&v.as_str()) {
            return Err(format!("mode takes fast, auto or thorough, not {v:?}"));
        }
        std::fs::write(proj.dir.join("mode"), format!("{v}\n")).map_err(|e| e.to_string())?;
    }
    say(&format!("mode = {}", config::load(&proj.root, None)?.get("mode").s()));
    Ok(())
}

/// One line for a status bar. Reads local files only.
fn cmd_line(g: &Global) -> R<()> {
    let proj = g.project();
    if !proj.initialized() {
        return Ok(());
    }
    let cfg = config::load(&proj.root, g.profile.as_deref())?;
    let s = proj.load_session();
    let daily = cfg.get("council").get("daily_tokens").n(0.0) as u64;
    let today = ledger::total_today(&cfg);
    let pct = if daily > 0 { format!("{}%", today * 100 / daily) } else { today.to_string() };
    let profile = match cfg.get("active_profile").s() { "" => "default", p => p };
    say(&format!(
        "{profile} · {} · {:.1}k · advisor {}/{} · today {pct}",
        cfg.get("mode").s(), s.spent_tokens as f64 / 1000.0, s.advisor.get("calls").n(0.0), cfg.get("advisor").get("max_calls").n(0.0)
    ));
    Ok(())
}

fn cmd_serve(g: &Global, args: &[String]) -> R<()> {
    let a = parse_args(args, &[("", "--port", true), ("", "--no-open", false)])?;
    let port = a.one("--port").map_or(Ok(8787), str::parse::<u16>).map_err(|_| "--port needs a number".to_string())?;
    let eng = g.engine(false)?;
    serve::serve(&eng.p, eng.cfg, port, !a.has("--no-open"))
}

fn run() -> R<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut g = Global { dir: None, profile: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-C" | "--dir" => {
                g.dir = Some(args.get(i + 1).ok_or("-C needs a directory")?.clone());
                i += 2;
            }
            "--profile" => {
                g.profile = Some(args.get(i + 1).ok_or("--profile needs a name")?.clone());
                i += 2;
            }
            "--version" | "-V" => {
                say(&format!("ctxflow {}", env!("CARGO_PKG_VERSION")));
                return Ok(());
            }
            "--help" | "-h" => {
                say(HELP);
                return Ok(());
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}. Run `ctxflow --help`.")),
            _ => break,
        }
    }
    let Some(cmd) = args.get(i) else {
        say(HELP);
        return Ok(());
    };
    let rest = &args[i + 1..];
    if let Some(d) = &g.dir {
        if !Path::new(d).is_dir() {
            return Err(format!("no such directory: {d}"));
        }
    }
    match cmd.as_str() {
        "init" => cmd_init(&g, rest),
        "ask" => cmd_ask(&g, rest),
        "prompt" => cmd_prompt(&g, rest),
        "chat" => cmd_chat(&g, rest),
        "status" => cmd_status(&g),
        "show" => cmd_show(&g, rest),
        "council" => cmd_council(&g, rest),
        "advise" => cmd_advise(&g, rest),
        "scout" => cmd_scout(&g, rest),
        "fold" => cmd_fold(&g),
        "progress" => cmd_progress(&g, rest),
        "handoff" => cmd_handoff(&g, rest),
        "new" => cmd_new(&g, rest),
        "profile" => cmd_profile(&g, rest),
        "mode" => cmd_mode(&g, rest),
        "line" => cmd_line(&g),
        "serve" => cmd_serve(&g, rest),
        "help" => {
            say(HELP);
            Ok(())
        }
        other => Err(format!("unknown command {other:?}. Run `ctxflow --help`.")),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ctxflow: {e}");
        std::process::exit(1);
    }
}
