"""Token estimation and context pruning.

Pruning runs in the kernel. Estimation is deliberately cheap and conservative (no network call): it is used
for *budget decisions*, while real usage numbers returned by the providers are
what get written to the ledger.
"""
from __future__ import annotations

import re

from . import kernel

_CJK = re.compile(r"[\u1100-\u11ff\u3040-\u30ff\u3130-\u318f\u3400-\u9fff\uac00-\ud7af]")


def estimate_tokens(text: str) -> int:
    """Rough token estimate: ~4 chars/token for Latin text, ~1.5 chars for CJK/Hangul."""
    if not text:
        return 0
    cjk = len(_CJK.findall(text))
    other = len(text) - cjk
    return int(other / 4 + cjk / 1.5) + 1


def prune_for_history(text: str, codeblock_max_lines: int, log_max_lines: int) -> tuple[str, list[dict]]:
    """Compress a message for history. Returns the text and the removed originals (stubs).

    Long code blocks and log dumps are useful for one turn, but re-sending them on every later
    turn burns the budget. They are replaced by `[#id ...]` references, and the caller stores
    the stubs, so nothing is lost: `ctxflow show <id>` returns the original bytes.
    """
    r = kernel.call("compress", text=text, code_max=codeblock_max_lines, log_max=log_max_lines)
    return r["text"], r["stubs"]


def truncate_to_tokens(text: str, max_tokens: int) -> tuple[str, bool]:
    if estimate_tokens(text) <= max_tokens:
        return text, False
    # binary search on character length
    lo, hi = 0, len(text)
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if estimate_tokens(text[:mid]) <= max_tokens:
            lo = mid
        else:
            hi = mid - 1
    return text[:lo] + "\n… [ctxflow: truncated to fit token budget]", True

