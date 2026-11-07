"""A stronger model on call. It reads a capped brief, never the transcript, and only when the gate opens."""
from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field

from . import backends, kernel
from .engine import Engine, log

_NEED = re.compile(r"^\s*NEED:\s*(.+)$", re.MULTILINE)
RESTORE_CAP_CHARS = 6000


@dataclass
class Advice:
    consulted: bool
    reason: str
    text: str = ""
    tokens: int = 0
    dropped: list[str] = field(default_factory=list)
    restored: list[str] = field(default_factory=list)


def _diff(root) -> str:
    try:
        out = subprocess.run(["git", "diff", "HEAD", "--stat", "-p"], cwd=root, capture_output=True,
                             text=True, timeout=20)
        return out.stdout if out.returncode == 0 else ""
    except (OSError, subprocess.SubprocessError):
        return ""


def advise(eng: Engine, trigger: str, question: str, error: str = "") -> Advice:
    s = eng.p.load_session()
    a = eng.cfg["advisor"]
    diff = _diff(eng.p.root) if trigger == "done" else ""
    # Review once per state of the code, not once per turn that ends with uncommitted work.
    diff_id = kernel.call("error_sig", text=diff)["sig"] if diff else ""
    changed = bool(diff) and diff_id != s.advisor.get("reviewed_diff")
    sig = kernel.call("error_sig", text=error or question)["sig"] if trigger == "error" else ""
    g = kernel.call("gate", trigger=trigger, state=s.advisor, error_sig=sig, has_changes=changed,
                    limits={"max_calls": a["max_calls"], "max_tokens": a["max_tokens"],
                            "error_repeat": a["error_repeat"]})
    s.advisor = g["state"]
    if trigger == "done" and g["consult"]:
        s.advisor["reviewed_diff"] = diff_id
    if not g["consult"]:
        eng.p.save_session(s)
        return Advice(False, g["reason"])

    recent = "\n\n".join(f"[user] {t.user}\n[assistant] {t.assistant}" for t in s.turns[-2:])
    sections = [{"name": "QUESTION", "text": f"Checkpoint: {g['reason']}\n{question}"}]
    if error:
        sections.append({"name": "ERROR", "text": error})
    sections += [
        {"name": "GOAL", "text": eng.p.read_doc("SPEC.md")},
        {"name": "CHANGES", "text": diff},
        {"name": "PROGRESS", "text": eng.p.read_doc("PROGRESS.md")},
        {"name": "SUMMARY", "text": s.summary},
        {"name": "RECENT", "text": recent},
    ]
    b = kernel.call("brief", cap=int(a["brief_tokens"]), sections=sections)
    eng.p.archive_put(b["stubs"])
    system = kernel.call("advisor_system")["text"]
    res = eng.route("advisor", system, b["brief"], kind=f"advisor/{trigger}", max_tokens=300)
    tokens = res.total_tokens
    restored: list[str] = []

    # One pull for what the brief left out. This is the only way it reads more.
    need = _NEED.search(res.text)
    if need:
        ids = re.findall(r"#?([0-9a-f]{12})", need.group(1))[:2]
        parts = []
        for i in ids:
            body = eng.p.archive_get(i)
            if body:
                restored.append(i)
                parts.append(f"=== #{i} ===\n{body[:RESTORE_CAP_CHARS]}")
        if parts:
            res = eng.route("advisor", system + "\nThe references you asked for are included. Decide now.",
                            b["brief"] + "\n\n" + "\n\n".join(parts), kind=f"advisor/{trigger}", max_tokens=300)
            tokens += res.total_tokens
    s.advisor["tokens"] = s.advisor.get("tokens", 0) + tokens
    s.spent_tokens += tokens
    eng.p.save_session(s)
    return Advice(True, g["reason"], res.text.strip(), tokens, b["dropped"], restored)


SCOUT_SYSTEM = """Look through the repository to answer the question, then report what you found.
You can only read. Reply with at most 15 lines. Give file:line references instead of pasting code.
State what you did not find. No preamble."""


def scout(eng: Engine, question: str) -> backends.Result:
    """A cheap read-only search that hands back a summary, so the search output never enters the context."""
    candidates = [c for c in eng.cfg["routing"].get("scout", []) if c.startswith("claude:")]
    if not candidates:
        raise SystemExit("ctxflow: scout needs a claude: model in routing.scout, because it needs file tools.")
    cfg = {**eng.cfg, "claude": {**eng.cfg["claude"], "extra_args": [
        "--tools", "Read,Grep,Glob", "--no-session-persistence", "--disable-slash-commands", "--strict-mcp-config"]}}
    log(f"scout → {candidates[0]} (read-only)")
    return backends.call(candidates[0], SCOUT_SYSTEM, question, cfg)
