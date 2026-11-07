"""Runs the council protocol. The kernel decides everything; this file only performs calls."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from typing import Any, Callable

from . import backends, config, kernel, ledger
from .engine import Engine, log


def preflight(eng: Engine, question: str, n: int) -> dict[str, int]:
    """Worst-case cost and how much of today's allowance remains. Makes no calls."""
    c = eng.cfg["council"]
    probe = kernel.call("council_step", start={
        "question": question, "seats": [{"model": "x", "title": "", "fallbacks": []}] * max(n, 2),
        "chair": {"model": "x", "fallbacks": []}, "options": [], "limits": {}})
    daily = int(c["daily_tokens"])
    left = max(0, daily - ledger.total_today()) if daily else 0
    return {"worst_case": int(probe["state"]["worst_case"]), "daily": daily, "left_today": left}


def _execute(eng: Engine, call: dict[str, Any]) -> dict[str, Any]:
    errors = []
    for spec in [call["model"], *call.get("fallbacks", [])]:
        reason, _ = ledger.check(eng.cfg, spec, len(call["prompt"]) // 4, int(call["max_tokens"]))
        if reason:
            errors.append(f"{spec}: {reason}")
            continue
        try:
            r = backends.call(spec, call["system"], call["prompt"], eng.cfg, int(call["max_tokens"]))
        except backends.BackendError as e:
            errors.append(f"{spec}: {e}")
            log(f"{spec} failed: {e}")
            if not e.retryable_elsewhere:
                break
            continue
        if r.estimated:
            r.input_tokens = r.input_tokens or len(call["prompt"]) // 4
            r.output_tokens = r.output_tokens or len(r.text) // 4
        ledger.record(spec, r.input_tokens, r.output_tokens, kind=f"council/{call['kind']}",
                      estimated=r.estimated, project=str(eng.p.root), cost_usd=r.extra.get("cost_usd"))
        return {"id": call["id"], "text": r.text, "model": spec,
                "in": r.input_tokens, "out": r.output_tokens, "redacted": r.extra.get("redacted", 0)}
    return {"id": call["id"], "error": "; ".join(errors) or "no model"}


def run(eng: Engine, question: str, options: list[str] | None = None,
        on_event: Callable[[dict[str, Any]], None] | None = None, mode: str | None = None) -> dict[str, Any]:
    seats, chair, fallbacks = config.council_seats(eng.cfg)
    c = eng.cfg["council"]
    limit = int(c["max_tokens"])
    pre = preflight(eng, question, len(seats))
    if pre["daily"]:
        if pre["left_today"] <= 0:
            raise SystemExit(f"ctxflow: today's limit of {pre['daily']} tokens is used up. "
                             "Use a local profile or raise [council] daily_tokens.")
        limit = min(limit, pre["left_today"])
    step = kernel.call("council_step", start={
        "question": question, "options": options or [],
        "seats": [{"model": m, "title": m, "lens": i, "fallbacks": fallbacks} for i, m in enumerate(seats)],
        "chair": {"model": chair, "fallbacks": fallbacks},
        "limits": {"mode": mode or eng.cfg.get("mode", "auto"), "max_tokens": limit, "agree_confidence": c["agree_confidence"], "clear_share": c["clear_share"]},
    })
    if step.get("error"):
        raise SystemExit(f"ctxflow: {step['error']}. This profile has too few seats.")
    redacted = 0
    while True:
        for ev in step.get("events", []):
            if on_event:
                on_event(ev)
        if step.get("done"):
            result = step["result"]
            result["redacted"] = redacted
            return result
        calls = step["calls"]
        with ThreadPoolExecutor(max_workers=max(1, len(calls))) as pool:
            results = list(pool.map(lambda x: _execute(eng, x), calls))
        redacted += sum(int(r.get("redacted", 0)) for r in results)
        step = kernel.call("council_step", state=step["state"], results=results)
