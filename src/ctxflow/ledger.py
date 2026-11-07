"""Usage ledger + budget guard.

The ledger is global (~/.ctxflow/usage.jsonl) because free-tier limits are per
API project / account, not per code repository.
"""
from __future__ import annotations

import json
import os
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from .config import model_budget


def ledger_path() -> Path:
    home = Path(os.environ.get("CTXFLOW_HOME", Path.home() / ".ctxflow"))
    home.mkdir(parents=True, exist_ok=True)
    return home / "usage.jsonl"


def record(spec: str, input_tokens: int, output_tokens: int, *, kind: str, estimated: bool,
           project: str, cost_usd: float | None = None) -> None:
    entry = {
        "ts": time.time(), "model": spec, "in": input_tokens, "out": output_tokens,
        "kind": kind, "est": estimated, "project": project,
    }
    if cost_usd is not None:
        entry["cost_usd"] = cost_usd
    with ledger_path().open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(entry, ensure_ascii=False) + "\n")


def _pacific_midnight_ts(now: float) -> float:
    """Gemini RPD quotas reset at midnight Pacific time."""
    try:
        from zoneinfo import ZoneInfo
        tz = ZoneInfo("America/Los_Angeles")
    except Exception:  # pragma: no cover - zoneinfo/tzdata missing
        tz = timezone(timedelta(hours=-8))
    local = datetime.fromtimestamp(now, tz)
    return local.replace(hour=0, minute=0, second=0, microsecond=0).timestamp()


def _entries_since(since: float) -> list[dict[str, Any]]:
    path = ledger_path()
    if not path.exists():
        return []
    out = []
    with path.open(encoding="utf-8") as fh:
        for line in fh:
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            if e.get("ts", 0) >= since:
                out.append(e)
    return out


def usage_window(spec: str | None = None, now: float | None = None) -> dict[str, dict[str, int]]:
    """Per-model counters: requests/tokens in the last minute and since the daily reset."""
    now = now or time.time()
    day_start = _pacific_midnight_ts(now)
    stats: dict[str, dict[str, int]] = {}
    for e in _entries_since(min(day_start, now - 60)):
        m = e["model"]
        if spec and m != spec:
            continue
        s = stats.setdefault(m, {"rpm": 0, "tpm": 0, "rpd": 0, "tpd": 0})
        tokens = int(e.get("in", 0)) + int(e.get("out", 0))
        if e["ts"] >= now - 60:
            s["rpm"] += 1
            s["tpm"] += int(e.get("in", 0))
        if e["ts"] >= day_start:
            s["rpd"] += 1
            s["tpd"] += tokens
    return stats


def total_today(now: float | None = None) -> int:
    """All tokens spent since the daily reset, across models. Local models cost nothing."""
    now = now or time.time()
    return sum(int(e.get("in", 0)) + int(e.get("out", 0))
               for e in _entries_since(_pacific_midnight_ts(now))
               if not str(e.get("model", "")).startswith(("ollama:", "mlx:", "lmstudio:", "echo:")))


def check(cfg: dict[str, Any], spec: str, est_input: int, est_output: int) -> tuple[str | None, bool]:
    """Return (reason, minute_only).

    reason is None if the call fits the budget. minute_only=True means only a
    per-minute limit (rpm/tpm) is hit, so waiting a bit is an option instead of
    falling back to another model.
    """
    limits = model_budget(cfg, spec)
    if not limits:
        return None, False
    ratio = float(cfg.get("budget", {}).get("safety_ratio", 0.9))
    used = usage_window(spec).get(spec, {"rpm": 0, "tpm": 0, "rpd": 0, "tpd": 0})
    planned = {"rpm": 1, "tpm": est_input, "rpd": 1, "tpd": est_input + est_output}
    hits = []
    for key in ("rpd", "tpd", "rpm", "tpm"):
        limit = int(limits.get(key, 0) or 0)
        if limit and used[key] + planned[key] > max(1, int(limit * ratio)):
            hits.append((key, f"{key} {used[key]}+{planned[key]} > {ratio:.0%} of {limit}"))
    if not hits:
        return None, False
    minute_only = all(k in ("rpm", "tpm") for k, _ in hits)
    return "; ".join(r for _, r in hits), minute_only
