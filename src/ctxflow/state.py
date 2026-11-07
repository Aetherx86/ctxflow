"""Project-local state: session (rolling summary + recent turns) and markdown state docs."""
from __future__ import annotations

import json
import shutil
import time
import uuid
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

from .config import CONFIG_DIR, CONFIG_FILE, DEFAULT_CONFIG_TOML

SPEC_TEMPLATE = """# SPEC

> 변하지 않는(또는 드물게 바뀌는) 목표·제약·설계 결정. 짧게 유지할수록 매 요청 토큰이 줄어듭니다.

## 목표
-

## 제약 / 기술 스택
-

## 핵심 설계 결정
-
"""

PROGRESS_TEMPLATE = """# PROGRESS

> 현재 상태의 단일 진실 공급원(Single Source of Truth). 채팅 기록 대신 이 문서가 '기억'입니다.

## 현재 상태
-

## 완료
-

## 다음 할 일
1.

## 알려진 이슈 / 메모
-
"""


@dataclass
class Turn:
    user: str
    assistant: str
    model: str
    ts: float = field(default_factory=time.time)


@dataclass
class Session:
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:8])
    started: float = field(default_factory=time.time)
    summary: str = ""
    turns: list[Turn] = field(default_factory=list)
    pending_progress: list[str] = field(default_factory=list)
    spent_tokens: int = 0
    advisor: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "Session":
        d = dict(d)
        d["turns"] = [Turn(**t) for t in d.get("turns", [])]
        return cls(**d)


class Project:
    def __init__(self, root: Path):
        self.root = root
        self.dir = root / CONFIG_DIR

    # ── discovery ────────────────────────────────────────────────────────────
    @staticmethod
    def find(start: Path) -> "Project":
        cur = start.resolve()
        for p in (cur, *cur.parents):
            # ~/.ctxflow holds the global ledger and is not a project, so the config file decides.
            if (p / CONFIG_DIR / "config.toml").is_file():
                return Project(p)
        return Project(cur)

    @property
    def initialized(self) -> bool:
        return (self.dir / "config.toml").is_file()

    def init(self, with_docs: bool = True) -> list[str]:
        created = []
        self.dir.mkdir(parents=True, exist_ok=True)
        (self.dir / "sessions").mkdir(exist_ok=True)
        (self.dir / "backups").mkdir(exist_ok=True)
        cfg = self.dir / CONFIG_FILE
        if not cfg.exists():
            cfg.write_text(DEFAULT_CONFIG_TOML.lstrip(), encoding="utf-8")
            created.append(str(cfg.relative_to(self.root)))
        gi = self.dir / ".gitignore"
        if not gi.exists():
            gi.write_text("session.json\nsessions/\nbackups/\narchive/\nprofile\nmode\n", encoding="utf-8")
        if with_docs:
            for name, tpl in (("SPEC.md", SPEC_TEMPLATE), ("PROGRESS.md", PROGRESS_TEMPLATE)):
                p = self.root / name
                if not p.exists():
                    p.write_text(tpl, encoding="utf-8")
                    created.append(name)
        return created

    # ── session ──────────────────────────────────────────────────────────────
    @property
    def session_path(self) -> Path:
        return self.dir / "session.json"

    def load_session(self) -> Session:
        if self.session_path.exists():
            return Session.from_dict(json.loads(self.session_path.read_text(encoding="utf-8")))
        return Session()

    def save_session(self, s: Session) -> None:
        self.dir.mkdir(parents=True, exist_ok=True)
        tmp = self.session_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(asdict(s), ensure_ascii=False, indent=2), encoding="utf-8")
        tmp.replace(self.session_path)

    def archive_session(self, s: Session) -> Path:
        dest = self.dir / "sessions" / f"{time.strftime('%Y%m%d-%H%M%S')}-{s.id}.json"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(json.dumps(asdict(s), ensure_ascii=False, indent=2), encoding="utf-8")
        return dest

    # ── markdown state docs ──────────────────────────────────────────────────
    # ── archive: originals of everything pruned from prompts, addressed by content hash ──
    def archive_put(self, stubs: list[dict[str, Any]]) -> None:
        if not stubs:
            return
        d = self.dir / "archive"
        d.mkdir(mode=0o700, exist_ok=True)
        for st in stubs:
            path = d / st["id"]
            if not path.exists():
                path.write_text(st["body"], encoding="utf-8")

    def archive_get(self, ref: str) -> str | None:
        ident = ref.lstrip("#")
        if not ident.isalnum():
            return None
        path = self.dir / "archive" / ident
        return path.read_text(encoding="utf-8") if path.is_file() else None

    def read_doc(self, name: str) -> str:
        p = self.root / name
        return p.read_text(encoding="utf-8") if p.exists() else ""

    def write_doc(self, name: str, content: str) -> None:
        p = self.root / name
        if p.exists():
            bdir = self.dir / "backups"
            bdir.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, bdir / f"{p.stem}.{time.strftime('%Y%m%d-%H%M%S')}{p.suffix}")
        p.write_text(content.rstrip() + "\n", encoding="utf-8")

