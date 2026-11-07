"""Core workflow: stateless prompt assembly, routing, rolling summary, handoff."""
from __future__ import annotations

import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from . import backends, kernel, ledger
from .state import Project, Session, Turn
from .tokens import estimate_tokens, prune_for_history, truncate_to_tokens

SYSTEM_PROMPT = """You are a senior engineer working inside a token-frugal, stateless workflow.
- You have NO memory between calls. Everything you know is in this prompt:
  state documents (SPEC/PROGRESS), a rolling summary, the last few turns, and attached files.
- Treat SPEC.md and PROGRESS.md as the source of truth. If the recent turns contradict them, say so.
- Be concise. Prefer diffs or the changed functions only, not whole files, unless asked.
- If your answer changes the project's state (something got done, a decision was made, a new
  issue/todo appeared), end with a section exactly titled `## PROGRESS DELTA` containing at most
  5 short bullets. Omit the section if nothing changed.
- [#id ...] marks an original stored outside this prompt. If you need one, say which id; the user
  can restore it. Never guess its content.
- Answer in the user's language."""

SUMMARY_SYSTEM = """You compress engineering conversations into dense, factual notes.
Keep: decisions, current status, file/function names, unresolved errors (one line each), next steps.
Drop: pleasantries, resolved error logs, full code, anything already in SPEC/PROGRESS.
Output only bullet lines, no preamble."""

DELTA_RE = re.compile(r"^##\s*PROGRESS DELTA\s*$(.*)", re.MULTILINE | re.DOTALL | re.IGNORECASE)


def log(msg: str) -> None:
    print(f"\033[2m[ctxflow] {msg}\033[0m", file=sys.stderr)


@dataclass
class Answer:
    text: str
    result: backends.Result
    task: str
    prompt_tokens: int


class Engine:
    def __init__(self, project: Project, cfg: dict[str, Any]):
        self.p = project
        self.cfg = cfg
        self.ctx = cfg["context"]

    # ── prompt assembly ──────────────────────────────────────────────────────
    def _attachments(self, files: list[str], force: bool) -> str:
        if not files:
            return ""
        limit = int(self.ctx["max_attach_files"])
        if len(files) > limit and not force:
            raise SystemExit(
                f"ctxflow: {len(files)} files attached but max_attach_files={limit}. "
                "Attach only the file you are changing (state docs carry the rest), or use --force."
            )
        per_file = max(500, int(self.ctx["max_attach_tokens"]) // len(files))
        parts = []
        for f in files:
            path = Path(f)
            if not path.is_file():
                raise SystemExit(f"ctxflow: no such file: {f}")
            body, cut = truncate_to_tokens(path.read_text(encoding="utf-8", errors="replace"), per_file)
            if cut:
                log(f"{f} truncated to ~{per_file} tokens")
            lang = path.suffix.lstrip(".")
            parts.append(f"=== ATTACHED FILE: {f} ===\n```{lang}\n{body}\n```")
        return "\n\n".join(parts)

    def build_prompt(self, s: Session, message: str, files: list[str], force: bool = False,
                     refs: list[str] | None = None) -> str:
        sections: list[str] = []
        for name in self.ctx["state_files"]:
            doc = self.p.read_doc(name).strip()
            if doc:
                sections.append(f"=== {name} ===\n{doc}")
        if s.summary:
            sections.append(f"=== ROLLING SUMMARY (older conversation) ===\n{s.summary}")
        if s.pending_progress:
            sections.append("=== PROGRESS DELTAS NOT YET MERGED INTO PROGRESS.md ===\n"
                            + "\n".join(s.pending_progress))
        if s.turns:
            recent = "\n\n".join(f"[user]\n{t.user}\n\n[assistant ({t.model})]\n{t.assistant}"
                                 for t in s.turns)
            sections.append(f"=== RECENT TURNS ===\n{recent}")
        att = self._attachments(files, force)
        if att:
            sections.append(att)
        for ref in refs or []:
            body = self.p.archive_get(ref)
            if body is None:
                raise SystemExit(f"ctxflow: nothing stored under {ref}")
            sections.append(f"=== RESTORED #{ref.lstrip('#')} ===\n{body}")
        sections.append(f"=== CURRENT REQUEST ===\n{message}")
        return "\n\n".join(sections)

    # ── routing ──────────────────────────────────────────────────────────────
    def classify(self, message: str, prompt_tokens: int) -> str:
        h = self.cfg["heuristics"]
        low = message.lower()
        if any(k.lower() in low for k in h["heavy_keywords"]):
            return "heavy"
        if prompt_tokens >= int(h["heavy_min_prompt_tokens"]):
            return "heavy"
        return "light"

    def route(self, task: str, system: str, prompt: str, *, kind: str,
              model: str | None = None, max_tokens: int | None = None) -> backends.Result:
        candidates = [model] if model else list(self.cfg["routing"][task])
        est_in = estimate_tokens(system + prompt)
        est_out = int(self.ctx["expected_output_tokens"])
        errors = []
        for spec in candidates:
            reason, minute_only = ledger.check(self.cfg, spec, est_in, est_out)
            if reason and minute_only and spec == candidates[-1]:
                log(f"{spec}: per-minute limit ({reason}); waiting 60s instead of failing")
                time.sleep(60)
                reason = None
            if reason:
                log(f"skip {spec}: budget ({reason})")
                errors.append(f"{spec}: {reason}")
                continue
            log(f"{kind} → {spec} (~{est_in} input tokens)")
            try:
                res = backends.call(spec, system, prompt, self.cfg, max_tokens)
            except backends.BackendError as e:
                log(f"{spec} failed: {e}")
                errors.append(f"{spec}: {e}")
                if e.retryable_elsewhere:
                    continue
                raise SystemExit(f"ctxflow: {e}")
            if res.estimated:
                res.input_tokens = res.input_tokens or est_in
                res.output_tokens = res.output_tokens or estimate_tokens(res.text)
                res.total_tokens = res.input_tokens + res.output_tokens
            ledger.record(spec, res.input_tokens, res.output_tokens, kind=kind,
                          estimated=res.estimated, project=str(self.p.root),
                          cost_usd=res.extra.get("cost_usd"))
            return res
        raise SystemExit("ctxflow: all candidates failed:\n  " + "\n  ".join(errors))

    # ── main operations ──────────────────────────────────────────────────────
    def ask(self, message: str, files: list[str] | None = None, *, task: str | None = None,
            model: str | None = None, force: bool = False, remember: bool = True,
            refs: list[str] | None = None) -> Answer:
        s = self.p.load_session()
        prompt = self.build_prompt(s, message, files or [], force, refs)
        ptoks = estimate_tokens(prompt)
        task = task or self.classify(message, ptoks)
        res = self.route(task, SYSTEM_PROMPT, prompt, kind=f"ask/{task}", model=model)

        if remember:
            self._remember(s, message, files or [], res)
        return Answer(text=res.text, result=res, task=task, prompt_tokens=ptoks)

    def _remember(self, s: Session, message: str, files: list[str], res: backends.Result) -> None:
        cb, lg = int(self.ctx["history_codeblock_max_lines"]), int(self.ctx["history_log_max_lines"])
        m = DELTA_RE.search(res.text)
        if m:
            s.pending_progress += [ln.strip() for ln in m.group(1).strip().splitlines() if ln.strip()]
        user_note = message + (f"\n(attached: {', '.join(files)})" if files else "")
        user, ustubs = prune_for_history(user_note, cb, lg)
        reply, rstubs = prune_for_history(DELTA_RE.sub("", res.text).strip(), cb, lg)
        self.p.archive_put(ustubs + rstubs)
        s.turns.append(Turn(user=user, assistant=reply, model=res.model))
        s.spent_tokens += res.total_tokens
        self._maybe_fold(s)
        self.p.save_session(s)
        if s.spent_tokens >= int(self.ctx["handoff_hint_tokens"]):
            log(f"session has consumed {s.spent_tokens} tokens — consider `ctxflow handoff`")

    def history_tokens(self, s: Session) -> int:
        return estimate_tokens(s.summary) + sum(estimate_tokens(t.user + t.assistant) for t in s.turns)

    def _maybe_fold(self, s: Session, force: bool = False) -> bool:
        keep = int(self.ctx["keep_turns"])
        too_many = len(s.turns) >= int(self.ctx["fold_when_turns"])
        too_big = self.history_tokens(s) > int(self.ctx["summarize_at_tokens"])
        if not (force or too_many or too_big) or len(s.turns) <= (0 if force else keep):
            return False
        cut = len(s.turns) - (0 if force else keep)
        old, s.turns = s.turns[:cut], s.turns[cut:]
        s.summary = self._summarize(s.summary, old)
        log(f"rolled {len(old)} old turn(s) into summary ({estimate_tokens(s.summary)} tokens)")
        return True

    def _summarize(self, prev: str, turns: list[Turn]) -> str:
        n = int(self.ctx["summary_max_lines"])
        convo = "\n\n".join(f"[user] {t.user}\n[assistant] {t.assistant}" for t in turns)
        prompt = (f"ROLLING SUMMARY TASK\nMerge the previous summary and the new turns into at most "
                  f"{n} bullet lines.\n\n=== PREVIOUS SUMMARY ===\n{prev or '(none)'}\n\n"
                  f"=== NEW TURNS ===\n{convo}")
        text = self.route("summary", SUMMARY_SYSTEM, prompt, kind="summary").text.strip()
        return self._keep_refs(text, f"{prev}\n{convo}")

    def _keep_refs(self, summary: str, source: str) -> str:
        """Models drop identifiers when they summarize. Put back any they lost."""
        pins = kernel.call("anchors", text=source, max=int(self.ctx["summary_max_refs"]))["anchors"]
        checked = kernel.call("verify_summary", text=summary, anchors=pins)
        if checked["missing"]:
            log(f"summary dropped {len(checked['missing'])} ref(s); restored them")
        return checked["text"]

    def fold_now(self) -> Session:
        s = self.p.load_session()
        if s.turns:
            self._maybe_fold(s, force=True)
            self.p.save_session(s)
        return s

    def update_progress(self, s: Session, extra: str = "") -> str | None:
        if not s.pending_progress and not extra:
            return None
        cur = self.p.read_doc("PROGRESS.md") or "# PROGRESS\n"
        prompt = ("PROGRESS UPDATE TASK\nRewrite PROGRESS.md by merging the deltas below. Keep the "
                  "same headings, move finished items to 완료/Done, keep it short (< 60 lines). "
                  "Output only the full markdown file.\n\n"
                  f"=== CURRENT PROGRESS.md ===\n{cur}\n\n=== DELTAS ===\n"
                  + "\n".join(s.pending_progress) + (f"\n{extra}" if extra else ""))
        text = self.route("summary", SUMMARY_SYSTEM.split("\n")[0], prompt, kind="progress").text
        text = re.sub(r"^```(?:markdown|md)?\s*\n|\n```\s*$", "", text.strip())
        self.p.write_doc("PROGRESS.md", text)
        s.pending_progress = []
        return text

    def handoff(self, note: str = "") -> tuple[str, Path]:
        """Summarize → update PROGRESS.md → write HANDOFF.md → archive → fresh session."""
        s = self.p.load_session()
        n = int(self.ctx["summary_max_lines"])
        convo = "\n\n".join(f"[user] {t.user}\n[assistant] {t.assistant}" for t in s.turns)
        prompt = (
            f"HANDOFF TASK\nWrite a handoff note (at most {n} bullet lines) so a brand-new session "
            "with zero memory can continue the work. Include: goal, what's done, current blocker, "
            "exact next step, files involved.\n\n"
            f"=== SPEC.md ===\n{self.p.read_doc('SPEC.md')}\n\n"
            f"=== PROGRESS.md ===\n{self.p.read_doc('PROGRESS.md')}\n\n"
            f"=== ROLLING SUMMARY ===\n{s.summary or '(none)'}\n\n"
            f"=== PENDING DELTAS ===\n" + "\n".join(s.pending_progress) + "\n\n"
            f"=== RECENT TURNS ===\n{convo or '(none)'}\n\n"
            + (f"=== USER NOTE ===\n{note}\n" if note else "")
        )
        summary = self.route("summary", SUMMARY_SYSTEM, prompt, kind="handoff").text.strip()
        summary = self._keep_refs(summary, f"{s.summary}\n{convo}\n" + "\n".join(s.pending_progress))

        self.update_progress(s, extra=note)
        hand = (f"# HANDOFF ({time.strftime('%Y-%m-%d %H:%M')}, session {s.id})\n\n"
                f"{summary}\n\n---\n새 세션/다른 앱에서 이어가려면: 이 문서 + SPEC.md + PROGRESS.md + "
                "수정할 파일 1개만 첨부하세요.\n")
        self.p.write_doc(self.ctx["handoff_file"], hand)
        archived = self.p.archive_session(s)
        self.p.save_session(Session(summary=summary))
        return hand, archived

