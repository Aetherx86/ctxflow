"""Loads kernel.wasm. All token-free decisions live there, so every host behaves identically."""
from __future__ import annotations

import json
import threading
from pathlib import Path
from typing import Any

from wasmtime import Instance, Module, Store

_inst: tuple[Store, Any] | None = None
_lock = threading.Lock()   # one store and one reset-per-call heap, while hosts call models in parallel


def _load() -> tuple[Store, Any]:
    global _inst
    if _inst is None:
        store = Store()
        module = Module.from_file(store.engine, str(Path(__file__).with_name("kernel.wasm")))
        _inst = (store, Instance(store, module, []).exports(store))
    return _inst


def call(op: str, **kw: Any) -> dict[str, Any]:
    data = json.dumps({"op": op, **kw}, ensure_ascii=False).encode()
    with _lock:
        store, ex = _load()
        ex["reset"](store)
        ptr = ex["alloc"](store, len(data))
        ex["memory"].write(store, data, ptr)
        packed = ex["call"](store, ptr, len(data))
        start, length = packed >> 32, packed & 0xFFFFFFFF
        out = bytes(ex["memory"].read(store, start, start + length))
    return json.loads(out.decode())
