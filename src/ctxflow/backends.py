"""LLM backends. Every call is fully stateless: one self-contained prompt in,
one text answer out. Conversation "memory" is assembled by ctxflow, not by the
provider — which is exactly what makes switching models mid-task free.
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any

from . import kernel


@dataclass
class Result:
    text: str
    model: str
    input_tokens: int = 0
    output_tokens: int = 0
    total_tokens: int = 0
    estimated: bool = False          # True when the provider gave no usage numbers
    extra: dict[str, Any] = field(default_factory=dict)


class BackendError(Exception):
    """Generic failure. `retryable_elsewhere` means: try the next model in the route."""

    def __init__(self, msg: str, *, rate_limited: bool = False, retryable_elsewhere: bool = True):
        super().__init__(msg)
        self.rate_limited = rate_limited
        self.retryable_elsewhere = retryable_elsewhere


def parse_spec(spec: str) -> tuple[str, str]:
    if ":" not in spec:
        raise ValueError(f"model spec must look like 'backend:model', got {spec!r}")
    backend, model = spec.split(":", 1)
    return backend.strip().lower(), model.strip()


def is_local(backend: str, cfg: dict[str, Any]) -> bool:
    return backend == "ollama" or bool(cfg.get("providers", {}).get(backend, {}).get("local"))


def call(spec: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    backend, model = parse_spec(spec)
    fn = {"gemini": _gemini, "claude": _claude, "ollama": _ollama, "anthropic": _anthropic,
          "echo": _echo}.get(backend)
    if fn is None and backend in cfg.get("providers", {}):
        fn = lambda m, s, p, c, mt: _openai_compat(backend, m, s, p, c, mt)  # noqa: E731
    if fn is None:
        raise BackendError(f"unknown backend {backend!r}", retryable_elsewhere=True)
    redacted = 0
    if not is_local(backend, cfg):
        r = kernel.call("redact", text=prompt)
        prompt, redacted = r["text"], len(r["map"])
    res = fn(model, system, prompt, cfg, max_tokens)
    res.text = _strip_think(res.text)
    res.extra["redacted"] = redacted
    return res


def _strip_think(text: str) -> str:
    """Reasoning models emit <think> blocks that are not part of the answer."""
    return re.sub(r"<think>.*?</think>\s*", "", text, flags=re.DOTALL)


# ── Gemini (Interactions API, REST, store=false) ─────────────────────────────

def _gemini(model: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    g = cfg["gemini"]
    key = os.environ.get(g["api_key_env"], "")
    if not key:
        raise BackendError(f"${g['api_key_env']} is not set")

    body: dict[str, Any] = {
        "model": model,
        "input": prompt,
        "store": False,                  # we keep state ourselves → no server-side history
        "system_instruction": system,
    }
    if g.get("thinking_level"):
        body["generation_config"] = {"thinking_level": g["thinking_level"]}
    if max_tokens:
        # Thinking counts against this limit, so leave room or the JSON gets cut off.
        body.setdefault("generation_config", {})["max_output_tokens"] = max(max_tokens * 3, 1024)

    req = urllib.request.Request(
        g["endpoint"],
        data=json.dumps(body).encode(),
        headers={
            "x-goog-api-key": key,
            "Content-Type": "application/json",
            "Api-Revision": g["api_revision"],
        },
        method="POST",
    )

    retries = int(g.get("max_retries", 2))
    for attempt in range(retries + 1):
        try:
            with urllib.request.urlopen(req, timeout=g.get("timeout_sec", 180)) as resp:
                data = json.loads(resp.read())
            break
        except urllib.error.HTTPError as e:
            detail = e.read().decode(errors="replace")[:500]
            if e.code == 429:
                # Free tier: a 429 usually means RPM/TPM/RPD exhausted. Back off once
                # for per-minute limits, then let the router fall through.
                if attempt < retries:
                    time.sleep(min(30, 5 * (attempt + 1)))
                    continue
                raise BackendError(f"gemini 429 rate limited: {detail}", rate_limited=True)
            if e.code >= 500 and attempt < retries:
                time.sleep(2 * (attempt + 1))
                continue
            raise BackendError(f"gemini HTTP {e.code}: {detail}")
        except (urllib.error.URLError, TimeoutError) as e:
            if attempt < retries:
                time.sleep(2 * (attempt + 1))
                continue
            raise BackendError(f"gemini network error: {e}")

    text = _gemini_text(data)
    usage = data.get("usage") or {}
    inp = int(usage.get("total_input_tokens", 0))
    out = int(usage.get("total_output_tokens", 0)) + int(usage.get("total_thought_tokens", 0))
    return Result(
        text=text,
        model=f"gemini:{model}",
        input_tokens=inp,
        output_tokens=out,
        total_tokens=int(usage.get("total_tokens", inp + out)),
        estimated=not usage,
    )


def _gemini_text(data: dict[str, Any]) -> str:
    if isinstance(data.get("output_text"), str):
        return data["output_text"]
    chunks: list[str] = []
    for step in data.get("steps", []):
        if step.get("type") == "model_output":
            for c in step.get("content", []):
                if c.get("type") == "text":
                    chunks.append(c.get("text", ""))
    if not chunks:
        raise BackendError(f"gemini returned no text (status={data.get('status')})")
    return "".join(chunks)


# ── Claude Code CLI (headless `claude -p`) ───────────────────────────────────

def _claude(model: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    c = cfg["claude"]
    binary = shutil.which(c.get("binary", "claude"))
    if not binary:
        raise BackendError("claude CLI not found on PATH")

    cmd = [binary, "-p", "--output-format", "json", "--system-prompt", system]
    if model and model != "default":
        cmd += ["--model", model]
    cmd += list(c.get("extra_args", []))

    try:
        proc = subprocess.run(
            cmd, input=prompt, capture_output=True, text=True,
            timeout=c.get("timeout_sec", 900),
        )
    except subprocess.TimeoutExpired:
        raise BackendError("claude timed out")

    raw = proc.stdout.strip()
    try:
        data = json.loads(raw.splitlines()[-1] if raw else "{}")
    except json.JSONDecodeError:
        data = {}

    if proc.returncode != 0 or data.get("is_error"):
        msg = data.get("result") or proc.stderr.strip() or raw or f"exit {proc.returncode}"
        limited = any(k in msg.lower() for k in ("usage limit", "rate limit", "limit reached", "429"))
        raise BackendError(f"claude failed: {msg[:500]}", rate_limited=limited)

    text = data.get("result", "")
    usage = data.get("usage") or {}
    inp = int(usage.get("input_tokens", 0)) + int(usage.get("cache_creation_input_tokens", 0)) \
        + int(usage.get("cache_read_input_tokens", 0))
    out = int(usage.get("output_tokens", 0))
    return Result(
        text=text,
        model=f"claude:{model}",
        input_tokens=inp,
        output_tokens=out,
        total_tokens=inp + out,
        estimated=not usage,
        extra={"cost_usd": data.get("total_cost_usd")},
    )


# ── Ollama (optional local model) ────────────────────────────────────────────

def _ollama_auto(host: str) -> str:
    try:
        with urllib.request.urlopen(host.rstrip("/") + "/api/tags", timeout=3) as resp:
            models = json.loads(resp.read()).get("models", [])
    except (urllib.error.URLError, TimeoutError, ValueError) as e:
        raise BackendError(f"ollama unavailable: {e}")
    if not models:
        raise BackendError("ollama has no models installed")
    return models[0]["name"]


def _ollama(model: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    o = cfg["ollama"]
    if model == "auto":
        model = _ollama_auto(o["host"])
    body: dict[str, Any] = {
        "model": model,
        "stream": False,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
    }
    if max_tokens:
        body["options"] = {"num_predict": max(max_tokens * 3, 1024)}
    req = urllib.request.Request(
        o["host"].rstrip("/") + "/api/chat",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=o.get("timeout_sec", 600)) as resp:
            data = json.loads(resp.read())
    except (urllib.error.URLError, TimeoutError) as e:
        raise BackendError(f"ollama unavailable: {e}")
    inp, out = int(data.get("prompt_eval_count", 0)), int(data.get("eval_count", 0))
    return Result(
        text=data.get("message", {}).get("content", ""),
        model=f"ollama:{model}", input_tokens=inp, output_tokens=out, total_tokens=inp + out,
    )


# ── Any OpenAI-compatible service, hosted or local (bring your own key) ──────

def _openai_compat(name: str, model: str, system: str, prompt: str, cfg: dict[str, Any],
                   max_tokens: int | None = None) -> Result:
    p = cfg["providers"][name]
    env = p.get("api_key_env", "")
    key = os.environ.get(env, "") if env else ""
    if env and not key:
        raise BackendError(f"${env} is not set")
    body: dict[str, Any] = {
        "model": model,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
    }
    if max_tokens:
        body["max_tokens"] = max(max_tokens * 3, 1024) if p.get("local") else max_tokens
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(p["base_url"].rstrip("/") + "/chat/completions",
                                 data=json.dumps(body).encode(), headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=p.get("timeout_sec", 300)) as resp:
            data = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        detail = e.read().decode(errors="replace")[:300]
        raise BackendError(f"{name} HTTP {e.code}: {detail}", rate_limited=e.code == 429)
    except (urllib.error.URLError, TimeoutError) as e:
        raise BackendError(f"{name} unavailable: {e}")
    choices = data.get("choices") or [{}]
    usage = data.get("usage") or {}
    inp, out = int(usage.get("prompt_tokens", 0)), int(usage.get("completion_tokens", 0))
    return Result(text=choices[0].get("message", {}).get("content") or "", model=f"{name}:{model}",
                  input_tokens=inp, output_tokens=out, total_tokens=inp + out, estimated=not usage)


# ── Anthropic API with the user's own credentials (optional: pip install anthropic) ──

def _anthropic(model: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    try:
        import anthropic
    except ImportError:
        raise BackendError("the anthropic: backend needs `uv tool install ctxflow --with anthropic`")
    kw: dict[str, Any] = {}
    effort = cfg.get("anthropic", {}).get("effort")
    if effort:
        kw["output_config"] = {"effort": effort}
    try:
        msg = anthropic.Anthropic().messages.create(
            model=model, max_tokens=max(max_tokens * 3, 1024) if max_tokens else 16000, system=system,
            messages=[{"role": "user", "content": prompt}], **kw)
    except anthropic.RateLimitError as e:
        raise BackendError(f"anthropic 429: {e}", rate_limited=True)
    except anthropic.APIStatusError as e:
        raise BackendError(f"anthropic HTTP {e.status_code}: {e.message}")
    except anthropic.AnthropicError as e:
        raise BackendError(f"anthropic: {e}")
    if msg.stop_reason == "refusal":
        raise BackendError("anthropic declined this request")
    u = msg.usage
    inp = u.input_tokens + (u.cache_read_input_tokens or 0) + (u.cache_creation_input_tokens or 0)
    return Result(text="".join(b.text for b in msg.content if b.type == "text"), model=f"anthropic:{model}",
                  input_tokens=inp, output_tokens=u.output_tokens, total_tokens=inp + u.output_tokens)


# ── echo (offline testing) ───────────────────────────────────────────────────

def _echo(model: str, system: str, prompt: str, cfg: dict[str, Any], max_tokens: int | None = None) -> Result:
    from .tokens import estimate_tokens

    if "ROLLING SUMMARY TASK" in prompt or "HANDOFF TASK" in prompt:
        text = "- [echo] summary line 1\n- [echo] summary line 2"
    elif "PROGRESS UPDATE TASK" in prompt:
        text = "# PROGRESS\n\n## 현재 상태\n- [echo] updated\n"
    else:
        text = f"[echo:{model}] received {estimate_tokens(prompt)} tokens. Last line: " \
               + prompt.strip().splitlines()[-1][:120]
    i, o = estimate_tokens(system + prompt), estimate_tokens(text)
    return Result(text=text, model=f"echo:{model}", input_tokens=i, output_tokens=o,
                  total_tokens=i + o, estimated=True)

