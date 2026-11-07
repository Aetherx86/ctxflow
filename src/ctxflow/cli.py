"""ctxflow command-line interface."""
from __future__ import annotations

import argparse
import json
import os
import select
import shutil
import sys
from pathlib import Path

from . import __version__, advisor, backends, config, council, ledger
from .engine import Engine
from .state import Project, Session
from .tokens import estimate_tokens


def _engine(args: argparse.Namespace, require_init: bool = True) -> Engine:
    proj = Project.find(Path(args.dir or "."))
    if require_init and not proj.initialized:
        raise SystemExit("ctxflow: not initialized here. Run `ctxflow init` in the project root.")
    return Engine(proj, config.load(proj.root, getattr(args, "profile", None)))


def _read_message(parts: list[str]) -> str:
    msg = " ".join(parts).strip()
    if not msg and not sys.stdin.isatty():
        msg = sys.stdin.read().strip()
    if not msg:
        raise SystemExit("ctxflow: empty message")
    return msg


def _print_answer(ans, quiet: bool) -> None:
    print(ans.text)
    if not quiet:
        r = ans.result
        flag = "~" if r.estimated else ""
        print(f"\n\033[2m── {r.model} · task={ans.task} · in {flag}{r.input_tokens} / out "
              f"{flag}{r.output_tokens} tokens\033[0m", file=sys.stderr)


# ── commands ─────────────────────────────────────────────────────────────────

def cmd_init(args):
    proj = Project.find(Path(args.dir or "."))
    if not proj.initialized:
        proj = Project(Path(args.dir or ".").resolve())
    created = proj.init(with_docs=not args.no_docs)
    print(f"ctxflow initialized at {proj.root}")
    for c in created:
        print(f"  + {c}")


def cmd_ask(args):
    eng = _engine(args)
    ans = eng.ask(_read_message(args.message), args.file, task=args.task, model=args.model,
                  force=args.force, remember=not args.no_remember, refs=args.ref)
    _print_answer(ans, args.quiet)


def cmd_prompt(args):
    """Dry run: print the exact stateless prompt (paste it into any web chat)."""
    eng = _engine(args)
    s = eng.p.load_session()
    msg = _read_message(args.message)
    prompt = eng.build_prompt(s, msg, args.file, args.force)
    ptoks = estimate_tokens(prompt)
    print(prompt)
    print(f"\n\033[2m── ~{ptoks} tokens · would route as task={eng.classify(msg, ptoks)}\033[0m",
          file=sys.stderr)


def cmd_status(args):
    eng = _engine(args, require_init=False)
    s = eng.p.load_session() if eng.p.initialized else Session()
    print(f"project : {eng.p.root}" + ("" if eng.p.initialized else "  (not initialized)"))
    print(f"session : {s.id} · {len(s.turns)} turn(s) kept · summary "
          f"{estimate_tokens(s.summary)} tok · history ~{eng.history_tokens(s)} tok · "
          f"spent {s.spent_tokens} tok · {len(s.pending_progress)} pending delta(s)")
    for name in eng.ctx["state_files"]:
        print(f"  {name:<12} ~{estimate_tokens(eng.p.read_doc(name))} tok")
    hint = int(eng.ctx["handoff_hint_tokens"])
    if s.spent_tokens >= hint:
        print(f"  ⚠ spent ≥ {hint}: run `ctxflow handoff` to start a fresh, cheap session")

    print("\nusage today (since 00:00 Pacific) / last 60s, vs. configured limits:")
    stats = ledger.usage_window()
    specs = sorted(set(stats) | {m for r in eng.cfg["routing"].values() for m in r})
    for spec in specs:
        u = stats.get(spec, {"rpm": 0, "tpm": 0, "rpd": 0, "tpd": 0})
        lim = config.model_budget(eng.cfg, spec)

        def fmt(k):
            limit = int(lim.get(k, 0) or 0)
            return f"{k}={u[k]}" + (f"/{limit}" if limit else "")
        print(f"  {spec:<34} " + "  ".join(fmt(k) for k in ("rpd", "tpd", "rpm", "tpm")))


def cmd_show(args):
    body = _engine(args).p.archive_get(args.id)
    if body is None:
        raise SystemExit(f"ctxflow: nothing stored under {args.id}")
    sys.stdout.write(body)


def cmd_council(args):
    eng = _engine(args)
    question = _read_message(args.message)
    _run_council(args, eng, question)


def _run_council(args, eng, question, *, auto=False):
    args.yes = getattr(args, "yes", False)
    args.json = getattr(args, "json", False)
    args.verbose = getattr(args, "verbose", False)
    pre = council.preflight(eng, question, len(config.council_seats(eng.cfg)[0]))
    over = pre["worst_case"] > int(eng.cfg["council"]["confirm_over"])
    print(f"\033[2m── at most ~{pre['worst_case']} tokens · limit {eng.cfg['council']['max_tokens']}"
          + (f" · {pre['left_today']} left today" if pre["daily"] else "") + "\033[0m", file=sys.stderr)
    if over and not args.yes:
        if auto:
            print("\033[2m── above [council] confirm_over, so it was not started automatically\033[0m", file=sys.stderr)
            return
        if not sys.stdin.isatty():
            raise SystemExit("ctxflow: above [council] confirm_over. Pass --yes to run.")
        if input("Run it? [y/N] ").strip().lower() != "y":
            return

    def on_event(ev):
        if ev["type"] == "stage" and not args.json:
            print(f"\033[2m── {ev['title']}\033[0m", file=sys.stderr)
        elif ev["type"] == "seat_error":
            print(f"\033[2m   seat {ev['seat']} failed: {ev['error']}\033[0m", file=sys.stderr)
        elif ev["type"] == "seat_end" and args.verbose:
            print(f"\n### {ev['model']}\n{ev['opinion']}")

    result = council.run(eng, question, getattr(args, "option", []), on_event)
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(result["final"])
        if result["matrix"]:
            print("\n| Model | Evidence | Risks | Feasible | Rank score | Flaws |\n|---|---|---|---|---|---|")
            for r in result["matrix"]:
                cell = lambda k, d=1: "-" if r[k] is None else f"{r[k]:.{d}f}"  # noqa: E731
                print(f"| {r['model']} | {cell('evidence')} | {cell('risk')} | {cell('feasibility')} "
                      f"| {cell('borda', 2)} | {len(r['fatal'])} |")
        u = result["used"]
        print(f"\n\033[2m── {result['mode']} · {int(result['independent_models'])} distinct model(s) · "
              f"{int(result['calls'])} calls · in {int(u['in'])} / out {int(u['out'])} tokens"
              + (f" · {result['redacted']} secret(s) removed" if result.get("redacted") else "")
              + "\033[0m", file=sys.stderr)
        if result["independent_models"] < 2 and result["mode"] != "failed":
            print("\033[2m── one model only, so this is not independent agreement\033[0m", file=sys.stderr)
    s = eng.p.load_session()
    s.spent_tokens += int(result["used"]["in"] + result["used"]["out"])
    s.pending_progress.append(f"- council ({result['mode']}): {question[:80]} -> {result['final'][:160]}")
    eng.p.save_session(s)


def _piped_input() -> str:
    """Reads stdin only if data is waiting. Hooks often leave it open and silent, and that must not hang."""
    try:
        if sys.stdin.isatty():
            return ""
        # Windows cannot select() on a pipe, so there it simply reads.
        if os.name != "nt" and not select.select([sys.stdin], [], [], 0.2)[0]:
            return ""
        return sys.stdin.read().strip()
    except (OSError, ValueError):
        return ""


def cmd_advise(args):
    eng = _engine(args)
    question = " ".join(args.message).strip()
    error = _piped_input()
    if args.trigger != "error" and error and not question:
        question, error = error, ""
    # Fast: only an explicit request reaches the stronger model.
    if eng.cfg["mode"] == "fast" and args.trigger != "manual":
        print("\033[2m── silent: Fast mode\033[0m", file=sys.stderr)
        return
    # Thorough: checkpoints that shape the work get the full council instead of one opinion.
    if eng.cfg["mode"] == "thorough" and args.trigger in ("plan", "done"):
        _run_council(args, eng, question or "Review the current state.", auto=True)
        return
    a = advisor.advise(eng, args.trigger, question or "Review the current state.", error)
    if not a.consulted:
        print(f"\033[2m── {a.reason}\033[0m", file=sys.stderr)
        return
    print(a.text)
    used = eng.p.load_session().advisor.get("calls", 0)
    print(f"\n\033[2m── {a.reason} · call {int(used)}/{eng.cfg['advisor']['max_calls']} · {a.tokens} tokens"
          + (f" · left out: {', '.join(a.dropped)}" if a.dropped else "")
          + (f" · restored: {', '.join(a.restored)}" if a.restored else "") + "\033[0m", file=sys.stderr)
    if a.text.startswith("ESCALATE"):
        if eng.cfg["mode"] == "fast":
            print("\033[2m── Fast mode: no council. Run `ctxflow council` to decide this\033[0m", file=sys.stderr)
        else:
            _run_council(args, eng, a.text.split(":", 1)[-1].strip() or question, auto=True)


def cmd_scout(args):
    eng = _engine(args)
    r = advisor.scout(eng, _read_message(args.message))
    print(r.text)
    print(f"\033[2m── {r.model} · in {r.input_tokens} / out {r.output_tokens} tokens\033[0m", file=sys.stderr)


def _detect() -> dict[str, bool]:
    host = "http://localhost:11434"
    try:
        backends._ollama_auto(host)
        local = True
    except backends.BackendError:
        local = False
    return {"gemini": bool(os.environ.get("GEMINI_API_KEY")), "local": local,
            "claude": bool(shutil.which("claude"))}


def cmd_profile(args):
    proj = Project.find(Path(args.dir or "."))
    if not proj.initialized:
        raise SystemExit("ctxflow: not initialized here. Run `ctxflow init` first.")
    cfg = config.load(proj.root)
    if args.action == "set":
        if args.name not in cfg["profiles"]:
            raise SystemExit(f"ctxflow: unknown profile. Available: {', '.join(cfg['profiles'])}")
        (proj.dir / "profile").write_text(args.name + "\n")
        print(f"profile = {args.name}")
        return
    if args.action == "clear":
        (proj.dir / "profile").unlink(missing_ok=True)
        print("profile cleared; using [routing] from config.toml")
        return
    d = _detect()
    print("detected: " + ", ".join(f"{k} {'yes' if v else 'no'}" for k, v in d.items()))
    need = {"local": ["local"], "free": ["gemini"], "balanced": ["gemini", "claude"], "claude": ["claude"]}
    for name, p in cfg["profiles"].items():
        mark = "*" if name == cfg["active_profile"] else " "
        ok = all(d[n] for n in need.get(name, []))
        print(f"{mark} {name:<10} {'ready ' if ok else '      '} {p.get('about', '')}")


def cmd_mode(args):
    proj = Project.find(Path(args.dir or "."))
    if not proj.initialized:
        raise SystemExit("ctxflow: not initialized here. Run `ctxflow init` first.")
    if args.value:
        (proj.dir / "mode").write_text(args.value + "\n")
    print(f"mode = {config.load(proj.root)['mode']}")


def cmd_serve(args):
    from .serve import serve
    serve(Project.find(Path(args.dir or ".")), config.load(Project.find(Path(args.dir or ".")).root, getattr(args, "profile", None)),
          args.port, not args.no_open)


def cmd_line(args):
    """One line for a status bar. Reads local files only."""
    proj = Project.find(Path(args.dir or "."))
    if not proj.initialized:
        return
    cfg = config.load(proj.root)
    s = proj.load_session()
    daily = int(cfg["council"]["daily_tokens"])
    today = ledger.total_today()
    pct = f"{today * 100 // daily}%" if daily else f"{today}"
    print(f"{cfg['active_profile'] or 'default'} · {cfg['mode']} · {s.spent_tokens / 1000:.1f}k · "
          f"advisor {int(s.advisor.get('calls', 0))}/{cfg['advisor']['max_calls']} · today {pct}")


def cmd_fold(args):
    eng = _engine(args)
    s = eng.fold_now()
    print(s.summary or "(nothing to fold)")


def cmd_progress(args):
    eng = _engine(args)
    s = eng.p.load_session()
    out = eng.update_progress(s, extra=" ".join(args.note))
    eng.p.save_session(s)
    print(out or "(no pending deltas — PROGRESS.md unchanged)")


def cmd_handoff(args):
    eng = _engine(args)
    text, archived = eng.handoff(note=" ".join(args.note))
    print(text)
    print(f"\033[2m── archived old session → {archived.relative_to(eng.p.root)}; "
          f"new session starts from the handoff summary\033[0m", file=sys.stderr)


def cmd_new(args):
    eng = _engine(args)
    old = eng.p.load_session()
    if old.turns or old.summary:
        eng.p.archive_session(old)
    eng.p.save_session(Session(summary="" if args.blank else old.summary))
    print("new session started" + ("" if args.blank else " (rolling summary carried over)"))


def cmd_chat(args):
    eng = _engine(args)
    print("ctxflow chat — /file <path>, /task light|heavy|auto, /model <spec|auto>, /fold, "
          "/progress, /handoff, /status, /quit")
    files: list[str] = []
    task, model = None, None
    while True:
        try:
            line = input("\n› ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            return
        if not line:
            continue
        if line.startswith("/"):
            cmd, _, rest = line[1:].partition(" ")
            rest = rest.strip()
            if cmd in ("q", "quit", "exit"):
                return
            elif cmd == "file":
                files = [rest] if rest else []
                print(f"attached for next message: {files or 'none'}")
            elif cmd == "task":
                task = None if rest in ("", "auto") else rest
                print(f"task = {task or 'auto'}")
            elif cmd == "model":
                model = None if rest in ("", "auto") else rest
                print(f"model = {model or 'auto (routing)'}")
            elif cmd == "fold":
                print(eng.fold_now().summary or "(nothing to fold)")
            elif cmd == "progress":
                s = eng.p.load_session()
                print(eng.update_progress(s) or "(no pending deltas)")
                eng.p.save_session(s)
            elif cmd == "handoff":
                print(eng.handoff(note=rest)[0])
            elif cmd == "status":
                cmd_status(args)
            else:
                print(f"unknown command /{cmd}")
            continue
        try:
            ans = eng.ask(line, files, task=task, model=model, force=args.force)
        except SystemExit as e:
            print(e, file=sys.stderr)
            continue
        files = []          # attachments are one-shot: never re-sent on later turns
        _print_answer(ans, False)


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser(prog="ctxflow", description=(
        "Stateless LLM workflow for free-tier token budgets: markdown state, context pruning, "
        "rolling summaries, handoffs and model routing (Gemini API + Claude Code)."))
    ap.add_argument("-C", "--dir", help="project directory (default: cwd, searched upward)")
    ap.add_argument("--profile", help="use this profile for one command")
    ap.add_argument("--version", action="version", version=f"ctxflow {__version__}")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("init", help="create .ctxflow/config.toml, SPEC.md, PROGRESS.md")
    p.add_argument("--no-docs", action="store_true")
    p.set_defaults(fn=cmd_init)

    def add_msg(p):
        p.add_argument("message", nargs="*", help="message (or pipe via stdin)")
        p.add_argument("-f", "--file", action="append", default=[], help="attach a file (one-shot)")
        p.add_argument("--force", action="store_true", help="allow more than max_attach_files")

    p = sub.add_parser("ask", help="one stateless request with assembled context")
    add_msg(p)
    p.add_argument("-t", "--task", choices=["light", "heavy", "summary"], help="override routing")
    p.add_argument("-m", "--model", help="force a model spec, e.g. gemini:gemini-3.8-flash, claude:opus")
    p.add_argument("--no-remember", action="store_true", help="don't store this turn")
    p.add_argument("-r", "--ref", action="append", default=[], help="restore a stored #id into this prompt")
    p.add_argument("-q", "--quiet", action="store_true")
    p.set_defaults(fn=cmd_ask)

    p = sub.add_parser("prompt", help="print the assembled prompt without calling a model")
    add_msg(p)
    p.set_defaults(fn=cmd_prompt)

    p = sub.add_parser("chat", help="interactive REPL")
    p.add_argument("--force", action="store_true")
    p.set_defaults(fn=cmd_chat)

    p = sub.add_parser("council", help="independent answers, anonymous scoring, one decision")
    p.add_argument("message", nargs="*")
    p.add_argument("-o", "--option", action="append", default=[], help="a fixed choice (repeat)")
    p.add_argument("-y", "--yes", action="store_true", help="skip the cost confirmation")
    p.add_argument("-v", "--verbose", action="store_true", help="print every answer")
    p.add_argument("--json", action="store_true")
    p.set_defaults(fn=cmd_council)

    p = sub.add_parser("advise", help="ask the stronger model, if the gate allows it")
    p.add_argument("trigger", choices=["plan", "error", "done", "manual"])
    p.add_argument("message", nargs="*")
    p.set_defaults(fn=cmd_advise)

    p = sub.add_parser("scout", help="cheap read-only repo search; returns a summary")
    p.add_argument("message", nargs="*")
    p.set_defaults(fn=cmd_scout)

    p = sub.add_parser("show", help="print a stored original by #id")
    p.add_argument("id")
    p.set_defaults(fn=cmd_show)

    p = sub.add_parser("profile", help="choose which providers to use")
    p.add_argument("action", nargs="?", choices=["list", "set", "clear"], default="list")
    p.add_argument("name", nargs="?")
    p.set_defaults(fn=cmd_profile)

    p = sub.add_parser("mode", help="fast: answers only. auto: the gate decides. thorough: full council")
    p.add_argument("value", nargs="?", choices=["fast", "auto", "thorough"])
    p.set_defaults(fn=cmd_mode)

    p = sub.add_parser("serve", help="open the app, with this machine's keys and models")
    p.add_argument("--port", type=int, default=8787)
    p.add_argument("--no-open", action="store_true")
    p.set_defaults(fn=cmd_serve)

    sub.add_parser("line", help="one status line (no model calls)").set_defaults(fn=cmd_line)
    sub.add_parser("status", help="session size + token usage vs. budgets").set_defaults(fn=cmd_status)
    sub.add_parser("fold", help="roll all kept turns into the summary now").set_defaults(fn=cmd_fold)

    p = sub.add_parser("progress", help="merge pending PROGRESS DELTAs into PROGRESS.md")
    p.add_argument("note", nargs="*")
    p.set_defaults(fn=cmd_progress)

    p = sub.add_parser("handoff", help="summary → PROGRESS.md → HANDOFF.md → fresh session")
    p.add_argument("note", nargs="*", help="optional note to include")
    p.set_defaults(fn=cmd_handoff)

    p = sub.add_parser("new", help="start a new session (keeps summary unless --blank)")
    p.add_argument("--blank", action="store_true")
    p.set_defaults(fn=cmd_new)

    args = ap.parse_args(argv)
    args.fn(args)


if __name__ == "__main__":
    main()
