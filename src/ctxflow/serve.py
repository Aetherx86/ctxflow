"""The app's local companion: serves the page and performs model calls with this machine's keys.

The page still runs the protocol in WASM. This only adds what a browser cannot do alone: reach
the Claude Code CLI and keep keys out of the browser. It listens on loopback only, and spends
money on the caller's behalf, so every API call needs the token from the start-up URL.
"""
from __future__ import annotations

import json
import mimetypes
import secrets
import threading
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from . import __version__, backends, ledger
from .state import Project

WEB = Path(__file__).with_name("web")
MAX_BODY = 8 * 1024 * 1024
mimetypes.add_type("application/wasm", ".wasm")
mimetypes.add_type("text/javascript", ".mjs")
mimetypes.add_type("application/manifest+json", ".webmanifest")


def _detect(cfg: dict[str, Any]) -> dict[str, bool]:
    import os
    import shutil
    try:
        backends._ollama_auto(cfg["ollama"]["host"])
        local = True
    except backends.BackendError:
        local = False
    return {"gemini": bool(os.environ.get(cfg["gemini"]["api_key_env"])), "local": local,
            "claude": bool(shutil.which(cfg["claude"].get("binary", "claude"))),
            "anthropic": bool(os.environ.get("ANTHROPIC_API_KEY")),
            "openrouter": bool(os.environ.get("OPENROUTER_API_KEY"))}


def make_handler(proj: Project, cfg: dict[str, Any], token: str, port: int):
    allowed_hosts = {f"127.0.0.1:{port}", f"localhost:{port}"}
    allowed_origins = {f"http://{h}" for h in allowed_hosts}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *a):  # requests can contain prompts; keep them out of the terminal
            pass

        def _send(self, code: int, body: bytes, ctype: str) -> None:
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.end_headers()
            self.wfile.write(body)

        def _json(self, code: int, data: Any) -> None:
            self._send(code, json.dumps(data, ensure_ascii=False).encode(), "application/json")

        def _guard(self) -> bool:
            # A hostile page can reach localhost. Host stops DNS rebinding, Origin stops cross-site
            # posts, and the token stops anything that was not given the start-up URL.
            if self.headers.get("Host") not in allowed_hosts:
                self._json(403, {"error": "bad host"})
                return False
            origin = self.headers.get("Origin")
            if origin and origin not in allowed_origins:
                self._json(403, {"error": "bad origin"})
                return False
            if self.path.startswith("/api/") and not secrets.compare_digest(self.headers.get("X-Ctxflow-Token", ""), token):
                self._json(401, {"error": "bad token"})
                return False
            return True

        def do_GET(self) -> None:
            if not self._guard():
                return
            path = urlparse(self.path).path
            if path == "/api/status":
                self._json(200, self._status())
                return
            rel = "index.html" if path == "/" else path.lstrip("/")
            target = (WEB / rel).resolve()
            if not target.is_relative_to(WEB.resolve()) or not target.is_file():
                self._json(404, {"error": "not found"})
                return
            self._send(200, target.read_bytes(), mimetypes.guess_type(target.name)[0] or "application/octet-stream")

        def _status(self) -> dict[str, Any]:
            c = cfg["council"]
            profiles = {k: {f: v.get(f) for f in ("about", "seats", "chair", "fallbacks")} for k, v in cfg["profiles"].items()}
            return {"version": __version__, "profiles": profiles, "active": cfg.get("active_profile", ""),
                    "mode": cfg.get("mode", "auto"), "detect": _detect(cfg), "today": ledger.total_today(),
                    "limits": {"max_tokens": c["max_tokens"], "confirm_over": c["confirm_over"], "daily_tokens": c["daily_tokens"]}}

        def do_POST(self) -> None:
            if not self._guard():
                return
            if urlparse(self.path).path != "/api/llm":
                self._json(404, {"error": "not found"})
                return
            length = int(self.headers.get("Content-Length") or 0)
            if length > MAX_BODY:
                self._json(413, {"error": "too large"})
                return
            try:
                req = json.loads(self.rfile.read(length))
                model, system, prompt = req["model"], req["system"], req["prompt"]
            except (ValueError, KeyError):
                self._json(400, {"error": "bad request"})
                return
            max_tokens = int(req.get("max_tokens") or 500)
            backend = backends.parse_spec(model)[0] if ":" in model else ""
            local = backends.is_local(backend, cfg)
            daily = int(cfg["council"]["daily_tokens"])
            if not local and daily and ledger.total_today() >= daily:
                self._json(429, {"error": f"today's limit of {daily} tokens is used up"})
                return
            reason, _ = ledger.check(cfg, model, len(prompt) // 4, max_tokens)
            if reason:
                self._json(429, {"error": reason})
                return
            try:
                r = backends.call(model, system, prompt, cfg, max_tokens)
            except (backends.BackendError, ValueError) as e:
                self._json(502, {"error": str(e)})
                return
            if r.estimated:
                r.input_tokens = r.input_tokens or len(prompt) // 4
                r.output_tokens = r.output_tokens or len(r.text) // 4
            ledger.record(model, r.input_tokens, r.output_tokens, kind=f"app/{req.get('kind', 'call')}",
                          estimated=r.estimated, project=str(proj.root), cost_usd=r.extra.get("cost_usd"))
            self._json(200, {"text": r.text, "input": r.input_tokens, "output": r.output_tokens,
                             "local": local, "removed": r.extra.get("redacted", 0)})

    return Handler


def serve(proj: Project, cfg: dict[str, Any], port: int, open_browser: bool = True, ready: threading.Event | None = None) -> None:
    token = secrets.token_urlsafe(24)
    srv = ThreadingHTTPServer(("127.0.0.1", port), make_handler(proj, cfg, token, port))
    url = f"http://127.0.0.1:{port}/?t={token}"
    print(f"ctxflow app: {url}\nPress Ctrl+C to stop.", flush=True)
    if open_browser:
        threading.Timer(0.3, webbrowser.open, [url]).start()
    if ready:
        ready.set()
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        srv.server_close()
