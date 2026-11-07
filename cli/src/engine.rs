//! Core workflow: stateless prompt assembly, routing, rolling summary, handoff.
use crate::backends::{self, Reply};
use crate::ledger;
use crate::state::{Project, Session, Turn};
use crate::util::{k, log, now, stamp, strs, truncate_to_tokens, R};
use ctxflow_kernel::compress::estimate;
use ctxflow_kernel::json::{st, V};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SYSTEM_PROMPT: &str = "You are a senior engineer working inside a token-frugal, stateless workflow.
- You have NO memory between calls. Everything you know is in this prompt:
  state documents (SPEC/PROGRESS), a rolling summary, the last few turns, and attached files.
- Treat SPEC.md and PROGRESS.md as the source of truth. If the recent turns contradict them, say so.
- Be concise. Prefer diffs or the changed functions only, not whole files, unless asked.
- If your answer changes the project's state (something got done, a decision was made, a new
  issue/todo appeared), end with a section exactly titled `## PROGRESS DELTA` containing at most
  5 short bullets. Omit the section if nothing changed.
- [#id ...] marks an original stored outside this prompt. If you need one, say which id; the user
  can restore it. Never guess its content.
- Answer in the user's language.";

const SUMMARY_SYSTEM: &str = "You compress engineering conversations into dense, factual notes.
Keep: decisions, current status, file/function names, unresolved errors (one line each), next steps.
Drop: pleasantries, resolved error logs, full code, anything already in SPEC/PROGRESS.
Output only bullet lines, no preamble.";

pub struct Answer {
    pub text: String,
    pub reply: Reply,
    pub task: String,
}

#[derive(Default)]
pub struct AskOptions<'a> {
    pub files: &'a [String],
    pub task: Option<&'a str>,
    pub model: Option<&'a str>,
    pub force: bool,
    pub remember: bool,
    pub refs: &'a [String],
}

/// Splits at a `## PROGRESS DELTA` heading: the text before it, and the delta after it.
fn split_delta(text: &str) -> (String, Option<String>) {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.trim_end().strip_prefix("##") {
            if rest.trim().eq_ignore_ascii_case("PROGRESS DELTA") {
                return (text[..offset].to_string(), Some(text[offset + line.len()..].to_string()));
            }
        }
        offset += line.len();
    }
    (text.to_string(), None)
}

fn strip_fence(text: &str) -> String {
    let t = text.trim();
    let t = ["```markdown", "```md", "```"].iter().find_map(|f| t.strip_prefix(f).and_then(|r| r.strip_prefix('\n').or(r.strip_prefix("\r\n")))).unwrap_or(t);
    t.strip_suffix("```").map_or(t, str::trim_end).to_string()
}

pub struct Engine {
    pub p: Project,
    pub cfg: V,
}

impl Engine {
    fn ctx(&self, key: &str, default: usize) -> usize {
        self.cfg.get("context").get(key).n(default as f64) as usize
    }

    // ── prompt assembly ──────────────────────────────────────────────────────────
    fn attachments(&self, files: &[String], force: bool) -> R<String> {
        if files.is_empty() {
            return Ok(String::new());
        }
        let limit = self.ctx("max_attach_files", 1);
        if files.len() > limit && !force {
            return Err(format!("{} files attached but max_attach_files={limit}. Attach only the file you are changing (state docs carry the rest), or use --force.", files.len()));
        }
        let per_file = (self.ctx("max_attach_tokens", 12000) / files.len()).max(500);
        let mut parts = Vec::new();
        for f in files {
            let path = Path::new(f);
            let bytes = std::fs::read(path).map_err(|_| format!("no such file: {f}"))?;
            let (body, cut) = truncate_to_tokens(&String::from_utf8_lossy(&bytes), per_file);
            if cut {
                log(&format!("{f} truncated to ~{per_file} tokens"));
            }
            let lang = path.extension().map_or_else(String::new, |e| e.to_string_lossy().into_owned());
            parts.push(format!("=== ATTACHED FILE: {f} ===\n```{lang}\n{body}\n```"));
        }
        Ok(parts.join("\n\n"))
    }

    pub fn build_prompt(&self, s: &Session, message: &str, files: &[String], force: bool, refs: &[String]) -> R<String> {
        let mut sections = Vec::new();
        for name in strs(self.cfg.get("context").get("state_files")) {
            let doc = self.p.read_doc(&name);
            if !doc.trim().is_empty() {
                sections.push(format!("=== {name} ===\n{}", doc.trim()));
            }
        }
        if !s.summary.is_empty() {
            sections.push(format!("=== ROLLING SUMMARY (older conversation) ===\n{}", s.summary));
        }
        if !s.pending_progress.is_empty() {
            sections.push(format!("=== PROGRESS DELTAS NOT YET MERGED INTO PROGRESS.md ===\n{}", s.pending_progress.join("\n")));
        }
        if !s.turns.is_empty() {
            let recent: Vec<String> = s.turns.iter().map(|t| format!("[user]\n{}\n\n[assistant ({})]\n{}", t.user, t.model, t.assistant)).collect();
            sections.push(format!("=== RECENT TURNS ===\n{}", recent.join("\n\n")));
        }
        let att = self.attachments(files, force)?;
        if !att.is_empty() {
            sections.push(att);
        }
        for reference in refs {
            let Some(body) = self.p.archive_get(reference) else {
                return Err(format!("nothing stored under {reference}"));
            };
            sections.push(format!("=== RESTORED #{} ===\n{body}", reference.trim_start_matches('#')));
        }
        sections.push(format!("=== CURRENT REQUEST ===\n{message}"));
        Ok(sections.join("\n\n"))
    }

    // ── routing ──────────────────────────────────────────────────────────────────
    pub fn classify(&self, message: &str, prompt_tokens: usize) -> &'static str {
        let h = self.cfg.get("heuristics");
        let low = message.to_lowercase();
        if strs(h.get("heavy_keywords")).iter().any(|w| low.contains(&w.to_lowercase())) {
            return "heavy";
        }
        if prompt_tokens >= h.get("heavy_min_prompt_tokens").n(5000.0) as usize {
            return "heavy";
        }
        "light"
    }

    pub fn route(&self, task: &str, system: &str, prompt: &str, kind: &str, model: Option<&str>, max_tokens: Option<u64>) -> R<Reply> {
        let candidates = model.map_or_else(|| strs(self.cfg.get("routing").get(task)), |m| vec![m.to_string()]);
        if candidates.is_empty() {
            return Err(format!("no model is configured for {task:?}. Check the profile or [routing]."));
        }
        let est_in = (estimate(system) + estimate(prompt)) as u64;
        let est_out = self.ctx("expected_output_tokens", 1500) as u64;
        let mut errors = Vec::new();
        for (i, spec) in candidates.iter().enumerate() {
            let (mut reason, minute_only) = ledger::check(&self.cfg, spec, est_in, est_out);
            if let (Some(r), true, true) = (&reason, minute_only, i + 1 == candidates.len()) {
                log(&format!("{spec}: per-minute limit ({r}); waiting 60s instead of failing"));
                std::thread::sleep(Duration::from_secs(60));
                reason = None;
            }
            if let Some(r) = reason {
                log(&format!("skip {spec}: budget ({r})"));
                errors.push(format!("{spec}: {r}"));
                continue;
            }
            log(&format!("{kind} → {spec} (~{est_in} input tokens)"));
            match backends::call(spec, system, prompt, &self.cfg, max_tokens) {
                Ok(mut res) => {
                    if res.estimated {
                        res.input_tokens = if res.input_tokens > 0 { res.input_tokens } else { est_in };
                        res.output_tokens = if res.output_tokens > 0 { res.output_tokens } else { estimate(&res.text) as u64 };
                        res.total_tokens = res.input_tokens + res.output_tokens;
                    }
                    ledger::record(spec, res.input_tokens, res.output_tokens, kind, res.estimated, &self.p.root.display().to_string(), res.cost_usd);
                    return Ok(res);
                }
                Err(e) => {
                    log(&format!("{spec} failed: {}{}", e.msg, if e.rate_limited { " (rate limited)" } else { "" }));
                    errors.push(format!("{spec}: {}", e.msg));
                }
            }
        }
        Err(format!("all candidates failed:\n  {}", errors.join("\n  ")))
    }

    // ── main operations ──────────────────────────────────────────────────────────
    pub fn ask(&self, message: &str, o: &AskOptions) -> R<Answer> {
        let mut s = self.p.load_session();
        let prompt = self.build_prompt(&s, message, o.files, o.force, o.refs)?;
        let prompt_tokens = estimate(&prompt);
        let task = o.task.unwrap_or_else(|| self.classify(message, prompt_tokens));
        let reply = self.route(task, SYSTEM_PROMPT, &prompt, &format!("ask/{task}"), o.model, None)?;
        if o.remember {
            self.remember(&mut s, message, o.files, &reply)?;
        }
        Ok(Answer { text: reply.text.clone(), reply, task: task.to_string() })
    }

    fn prune(&self, text: &str) -> R<String> {
        let r = k("compress", vec![
            ("text", st(text)),
            ("code_max", V::Num(self.ctx("history_codeblock_max_lines", 12) as f64)),
            ("log_max", V::Num(self.ctx("history_log_max_lines", 6) as f64)),
        ]);
        self.p.archive_put(r.get("stubs").arr())?;
        Ok(r.get("text").s().to_string())
    }

    fn remember(&self, s: &mut Session, message: &str, files: &[String], reply: &Reply) -> R<()> {
        let (body, delta) = split_delta(&reply.text);
        if let Some(d) = delta {
            s.pending_progress.extend(d.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string));
        }
        let note = if files.is_empty() { message.to_string() } else { format!("{message}\n(attached: {})", files.join(", ")) };
        s.turns.push(Turn { user: self.prune(&note)?, assistant: self.prune(body.trim())?, model: reply.model.clone(), ts: now() });
        s.spent_tokens += reply.total_tokens;
        self.maybe_fold(s, false)?;
        self.p.save_session(s)?;
        let hint = self.ctx("handoff_hint_tokens", 60000) as u64;
        if s.spent_tokens >= hint {
            log(&format!("session has consumed {} tokens — consider `ctxflow handoff`", s.spent_tokens));
        }
        Ok(())
    }

    pub fn history_tokens(&self, s: &Session) -> usize {
        estimate(&s.summary) + s.turns.iter().map(|t| estimate(&format!("{}{}", t.user, t.assistant))).sum::<usize>()
    }

    fn maybe_fold(&self, s: &mut Session, force: bool) -> R<bool> {
        let keep = if force { 0 } else { self.ctx("keep_turns", 3) };
        let too_many = s.turns.len() >= self.ctx("fold_when_turns", 6);
        let too_big = self.history_tokens(s) > self.ctx("summarize_at_tokens", 6000);
        if !(force || too_many || too_big) || s.turns.len() <= keep {
            return Ok(false);
        }
        let rest = s.turns.split_off(s.turns.len() - keep);
        let old = std::mem::replace(&mut s.turns, rest);
        s.summary = self.summarize(&s.summary, &old)?;
        log(&format!("rolled {} old turn(s) into summary ({} tokens)", old.len(), estimate(&s.summary)));
        Ok(true)
    }

    fn summarize(&self, prev: &str, turns: &[Turn]) -> R<String> {
        let n = self.ctx("summary_max_lines", 10);
        let convo = convo_text(turns);
        let prompt = format!("ROLLING SUMMARY TASK\nMerge the previous summary and the new turns into at most {n} bullet lines.\n\n=== PREVIOUS SUMMARY ===\n{}\n\n=== NEW TURNS ===\n{convo}", if prev.is_empty() { "(none)" } else { prev });
        let text = self.route("summary", SUMMARY_SYSTEM, &prompt, "summary", None, None)?.text;
        Ok(self.keep_refs(text.trim(), &format!("{prev}\n{convo}")))
    }

    /// Models drop identifiers when they summarize. Put back any they lost.
    fn keep_refs(&self, summary: &str, source: &str) -> String {
        let pins = k("anchors", vec![("text", st(source)), ("max", V::Num(self.ctx("summary_max_refs", 30) as f64))]);
        let checked = k("verify_summary", vec![("text", st(summary)), ("anchors", pins.get("anchors").clone())]);
        let missing = checked.get("missing").arr().len();
        if missing > 0 {
            log(&format!("summary dropped {missing} ref(s); restored them"));
        }
        checked.get("text").s().to_string()
    }

    pub fn fold_now(&self) -> R<Session> {
        let mut s = self.p.load_session();
        if !s.turns.is_empty() {
            self.maybe_fold(&mut s, true)?;
            self.p.save_session(&s)?;
        }
        Ok(s)
    }

    pub fn update_progress(&self, s: &mut Session, extra: &str) -> R<Option<String>> {
        if s.pending_progress.is_empty() && extra.is_empty() {
            return Ok(None);
        }
        let mut cur = self.p.read_doc("PROGRESS.md");
        if cur.is_empty() {
            cur = "# PROGRESS\n".to_string();
        }
        let prompt = format!("PROGRESS UPDATE TASK\nRewrite PROGRESS.md by merging the deltas below. Keep the same headings, move finished items to Done, keep it short (< 60 lines). Output only the full markdown file.\n\n=== CURRENT PROGRESS.md ===\n{cur}\n\n=== DELTAS ===\n{}{}", s.pending_progress.join("\n"), if extra.is_empty() { String::new() } else { format!("\n{extra}") });
        let first_line = SUMMARY_SYSTEM.lines().next().unwrap_or("");
        let text = strip_fence(&self.route("summary", first_line, &prompt, "progress", None, None)?.text);
        self.p.write_doc("PROGRESS.md", &text)?;
        s.pending_progress.clear();
        Ok(Some(text))
    }

    /// Summarize, update PROGRESS.md, write HANDOFF.md, archive, and start a fresh session.
    pub fn handoff(&self, note: &str) -> R<(String, PathBuf)> {
        let mut s = self.p.load_session();
        let n = self.ctx("summary_max_lines", 10);
        let convo = convo_text(&s.turns);
        let prompt = format!(
            "HANDOFF TASK\nWrite a handoff note (at most {n} bullet lines) so a brand-new session with zero memory can continue the work. Include: goal, what's done, current blocker, exact next step, files involved.\n\n=== SPEC.md ===\n{}\n\n=== PROGRESS.md ===\n{}\n\n=== ROLLING SUMMARY ===\n{}\n\n=== PENDING DELTAS ===\n{}\n\n=== RECENT TURNS ===\n{}\n\n{}",
            self.p.read_doc("SPEC.md"),
            self.p.read_doc("PROGRESS.md"),
            if s.summary.is_empty() { "(none)" } else { &s.summary },
            s.pending_progress.join("\n"),
            if convo.is_empty() { "(none)" } else { &convo },
            if note.is_empty() { String::new() } else { format!("=== USER NOTE ===\n{note}\n") },
        );
        let summary = self.route("summary", SUMMARY_SYSTEM, &prompt, "handoff", None, None)?.text;
        let source = format!("{}\n{convo}\n{}", s.summary, s.pending_progress.join("\n"));
        let summary = self.keep_refs(summary.trim(), &source);
        self.update_progress(&mut s, note)?;
        let hand = format!("# HANDOFF ({} UTC, session {})\n\n{summary}\n\n---\nTo continue in a new session or another app, attach only this file, SPEC.md, PROGRESS.md and the one file you are changing.\n", stamp(" "), s.id);
        let file = self.cfg.get("context").get("handoff_file").s();
        self.p.write_doc(if file.is_empty() { "HANDOFF.md" } else { file }, &hand)?;
        let archived = self.p.archive_session(&s)?;
        self.p.save_session(&Session::new(&summary))?;
        Ok((hand, archived))
    }
}

fn convo_text(turns: &[Turn]) -> String {
    turns.iter().map(|t| format!("[user] {}\n[assistant] {}", t.user, t.assistant)).collect::<Vec<_>>().join("\n\n")
}

/// The configured models, for `status`.
pub fn routed_models(cfg: &V) -> Vec<String> {
    let V::Obj(routes) = cfg.get("routing") else { return Vec::new() };
    routes.iter().flat_map(|(_, v)| strs(v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_is_split_at_its_heading() {
        let (a, d) = split_delta("done.\n\n## PROGRESS DELTA\n- one\n- two");
        assert_eq!(a.trim(), "done.");
        assert_eq!(d.as_deref(), Some("- one\n- two"));
        let (a, d) = split_delta("## progress delta  \n- x\n");
        assert_eq!(a, "");
        assert_eq!(d.as_deref(), Some("- x\n"));
        assert!(split_delta("## Other\ntext").1.is_none());
    }

    #[test]
    fn fences_are_removed_from_rewritten_docs() {
        assert_eq!(strip_fence("```markdown\n# A\n```"), "# A");
        assert_eq!(strip_fence("```\n# A\n```\n"), "# A");
        assert_eq!(strip_fence("# A"), "# A");
    }
}
