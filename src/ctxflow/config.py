"""Configuration loading for ctxflow.

The config lives in `<project>/.ctxflow/config.toml`. Anything the user omits
falls back to DEFAULT_CONFIG_TOML, so a partial config file is fine.
"""
from __future__ import annotations

import copy
import tomllib
from pathlib import Path
from typing import Any

DEFAULT_CONFIG_TOML = r'''
# ─────────────────────────────────────────────────────────────────────────────
# ctxflow 설정
# 모델 스펙 형식: "<backend>:<model>"
#   gemini | anthropic (자기 API 키) | claude (로그인된 Claude Code CLI)
#   ollama | mlx | lmstudio (로컬, 비밀값 제거 생략)
#   openai | openrouter | groq | deepseek | [providers.*]에 직접 추가한 OpenAI 호환 서비스
#   ollama:auto = 설치된 첫 번째 Ollama 모델
# ─────────────────────────────────────────────────────────────────────────────

[routing]
# 작업 유형별 후보 목록. 앞에서부터 시도하고, 예산 초과/에러면 다음 후보로 폴백.
# claude:haiku 는 GEMINI_API_KEY 미설정·Gemini 한도 소진 시의 마지막 안전망.
light   = ["gemini:gemini-3.5-flash-lite", "gemini:gemini-3.1-flash-lite", "claude:haiku"]
summary = ["gemini:gemini-3.5-flash-lite", "gemini:gemini-3.1-flash-lite", "claude:haiku"]
heavy   = ["claude:sonnet", "gemini:gemini-3.8-flash"]
advisor = ["claude:opus", "gemini:gemini-3.8-flash"]
scout   = ["claude:haiku"]

[context]
keep_turns = 3                      # 롤링 요약 후 원문(압축본)으로 남길 최근 턴 수
fold_when_turns = 6                 # 저장된 턴이 이 수에 도달하면 오래된 턴을 요약으로 접음
summarize_at_tokens = 6000          # 또는 히스토리 추정 토큰이 이 값을 넘으면 접음
summary_max_lines = 10
summary_max_refs = 30               # 요약에서 빠지면 코드가 다시 붙이는 식별자 수
max_attach_files = 1                # 한 번에 첨부할 코드 파일 수 (--force 로 무시)
max_attach_tokens = 12000           # 첨부 파일 합계 상한, 넘으면 잘라냄
history_codeblock_max_lines = 12    # 히스토리 저장 시 이보다 긴 코드블록은 생략 표시로 치환
history_log_max_lines = 6           # 연속된 로그/스택트레이스 줄이 이보다 많으면 압축
state_files = ["SPEC.md", "PROGRESS.md"]
handoff_file = "HANDOFF.md"
handoff_hint_tokens = 60000         # 세션 누적 소비 토큰이 이를 넘으면 핸드오프 권장
expected_output_tokens = 1500       # 예산 검사 시 응답 크기 추정치

[heuristics]
heavy_keywords = [
  "설계", "아키텍처", "디버그", "디버깅", "버그", "원인", "리팩터", "리팩토링", "보안", "성능",
  "design", "architecture", "debug", "root cause", "refactor", "race", "deadlock",
  "security", "performance", "why does", "why is",
]
heavy_min_prompt_tokens = 5000      # 조립된 프롬프트가 이보다 크면 heavy 로 분류

[gemini]
api_key_env = "GEMINI_API_KEY"
endpoint = "https://generativelanguage.googleapis.com/v1beta/interactions"
api_revision = "2026-05-20"
thinking_level = "low"              # 빈 문자열이면 생략(모델 기본값)
max_retries = 2
timeout_sec = 180

[council]
max_tokens = 20000                  # 실행당 상한. 넘으면 다음 단계를 건너뛰고 그때까지의 최선을 돌려줌
confirm_over = 15000                # 최악의 경우 추정치가 이보다 크면 실행 전에 확인
daily_tokens = 200000               # 오늘 전체 사용량 상한 (0 = 무제한)
agree_confidence = 70               # 모두 같은 답이고 확신도가 이 이상이면 채점 생략
clear_share = 0.75                  # 채점자 중 이 비율이 1위로 꼽으면 최종 모델 호출 생략

[advisor]
max_calls = 3                       # 세션당 호출 수
max_tokens = 6000                   # 세션당 어드바이저 토큰
brief_tokens = 1500                 # 어드바이저에게 보내는 요약본 상한 (전체 대화는 보내지 않음)
error_repeat = 2                    # 같은 에러가 이만큼 반복되어야 호출

# 프로파일: `ctxflow profile set <이름>`. 고르지 않으면 위의 [routing]을 그대로 사용.
[profiles.local]
about = "키 없음. 로컬 모델만 사용 (비용 0, 모델이 하나면 교차 검증 효과는 약함)"
seats = ["ollama:auto", "ollama:auto", "ollama:auto"]
chair = "ollama:auto"
fallbacks = []
routing = { light = ["ollama:auto"], summary = ["ollama:auto"], heavy = ["ollama:auto"], advisor = ["ollama:auto"], scout = [] }

[profiles.free]
about = "Gemini 무료 티어 + 있으면 로컬 모델"
seats = ["gemini:gemini-3.5-flash-lite", "gemini:gemini-3-flash-preview", "gemini:gemini-3.5-flash", "ollama:auto"]
chair = "gemini:gemini-3.5-flash"
fallbacks = ["gemini:gemini-3.1-flash-lite"]
routing = { light = ["gemini:gemini-3.5-flash-lite", "gemini:gemini-3.1-flash-lite"], summary = ["gemini:gemini-3.5-flash-lite", "gemini:gemini-3.1-flash-lite"], heavy = ["gemini:gemini-3.8-flash", "gemini:gemini-3.5-flash"], advisor = ["gemini:gemini-3.8-flash", "gemini:gemini-3.5-flash"], scout = [] }

[profiles.balanced]
about = "저가 모델이 일하고 상위 모델은 게이트를 통과할 때만 호출 (Gemini + Claude Code)"
seats = ["gemini:gemini-3.5-flash-lite", "claude:haiku", "gemini:gemini-3.5-flash", "ollama:auto"]
chair = "claude:sonnet"
fallbacks = ["gemini:gemini-3.1-flash-lite", "claude:haiku"]
routing = { light = ["gemini:gemini-3.5-flash-lite", "claude:haiku"], summary = ["gemini:gemini-3.5-flash-lite", "claude:haiku"], heavy = ["claude:sonnet", "gemini:gemini-3.8-flash"], advisor = ["claude:opus", "gemini:gemini-3.8-flash"], scout = ["claude:haiku"] }

[profiles.claude]
about = "Claude Code 로그인만 사용"
seats = ["claude:haiku", "claude:sonnet"]
chair = "claude:opus"
fallbacks = []
routing = { light = ["claude:haiku"], summary = ["claude:haiku"], heavy = ["claude:sonnet"], advisor = ["claude:opus"], scout = ["claude:haiku"] }

[profiles.api]
about = "자기 API 키만 사용 (ANTHROPIC_API_KEY, OPENROUTER_API_KEY)"
seats = ["anthropic:claude-haiku-5-5", "openrouter:google/gemini-3.5-flash", "openrouter:deepseek/deepseek-chat", "ollama:auto"]
chair = "anthropic:claude-opus-5-5"
fallbacks = []
routing = { light = ["anthropic:claude-haiku-5-5"], summary = ["anthropic:claude-haiku-5-5"], heavy = ["anthropic:claude-sonnet-5-5"], advisor = ["anthropic:claude-opus-5-5"], scout = [] }

[anthropic]
effort = "low"                      # 짧은 호출에 사고 토큰을 쓰지 않도록. 빈 문자열이면 모델 기본값

[providers.openai]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[providers.openrouter]
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"

[providers.groq]
base_url = "https://api.groq.com/openai/v1"
api_key_env = "GROQ_API_KEY"

[providers.deepseek]
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"

[providers.lmstudio]
base_url = "http://127.0.0.1:1234/v1"
api_key_env = ""
local = true

[providers.mlx]
base_url = "http://127.0.0.1:8080/v1"
api_key_env = ""
local = true

[claude]
binary = "claude"
# --tools "" : 도구 없이 순수 LLM 응답만 (토큰 절약). 파일 편집까지 맡기려면 지우세요.
# --disable-slash-commands : 스킬/커맨드 목록 주입 제거 → 호출당 ~3.4k → ~0.6k 입력 토큰 (실측)
# --strict-mcp-config : 전역 MCP 서버 도구 스키마 주입 방지
extra_args = ["--tools", "", "--no-session-persistence", "--disable-slash-commands", "--strict-mcp-config"]
timeout_sec = 900

[ollama]
host = "http://localhost:11434"
timeout_sec = 600

[budget]
safety_ratio = 0.9                  # 한도의 90% 에 닿으면 다음 후보로 넘어감

# 모델별 자체 상한. 0 또는 미지정 = 무제한.
# ⚠️ 아래 숫자는 보수적인 예시입니다. 실제 무료 티어 한도는
#    https://aistudio.google.com/rate-limit 에서 확인 후 수정하세요.
# rpd/tpd 는 태평양 시간 자정 기준(Gemini RPD 리셋 기준)으로 집계합니다.
[budget.models."gemini:gemini-3.5-flash-lite"]
rpm = 15
tpm = 250000
rpd = 1000

[budget.models."gemini:gemini-3.1-flash-lite"]
rpm = 15
tpm = 250000
rpd = 1000

[budget.models."gemini:gemini-3.8-flash"]
rpm = 10
tpm = 250000
rpd = 250

[budget.models."claude:sonnet"]
rpm = 0
tpd = 0
'''

CONFIG_DIR = ".ctxflow"
CONFIG_FILE = "config.toml"


def _deep_merge(base: dict[str, Any], over: dict[str, Any]) -> dict[str, Any]:
    out = copy.deepcopy(base)
    for key, val in over.items():
        if isinstance(val, dict) and isinstance(out.get(key), dict):
            out[key] = _deep_merge(out[key], val)
        else:
            out[key] = val
    return out


def defaults() -> dict[str, Any]:
    return tomllib.loads(DEFAULT_CONFIG_TOML)


def load(root: Path, profile: str | None = None) -> dict[str, Any]:
    cfg = defaults()
    path = root / CONFIG_DIR / CONFIG_FILE
    if path.exists():
        with path.open("rb") as fh:
            cfg = _deep_merge(cfg, tomllib.load(fh))
    marker = root / CONFIG_DIR / "profile"
    name = profile or (marker.read_text().strip() if marker.exists() else "")
    if name:
        if name not in cfg["profiles"]:
            raise SystemExit(f"ctxflow: unknown profile {name!r}. Available: {', '.join(cfg['profiles'])}")
        cfg["routing"] = {**cfg["routing"], **cfg["profiles"][name].get("routing", {})}
    cfg["active_profile"] = name
    mode_file = root / CONFIG_DIR / "mode"
    cfg["mode"] = mode_file.read_text().strip() if mode_file.exists() else "auto"
    return cfg


def council_seats(cfg: dict[str, Any]) -> tuple[list[str], str, list[str]]:
    """Seats, chair and fallbacks of the active profile, or a cheap default from [routing]."""
    prof = cfg["profiles"].get(cfg.get("active_profile") or "")
    if prof:
        return list(prof["seats"]), prof["chair"], list(prof.get("fallbacks", []))
    light, heavy = cfg["routing"]["light"], cfg["routing"]["heavy"]
    seats = list(dict.fromkeys(light + heavy))[:4]
    return seats, heavy[0], []


def model_budget(cfg: dict[str, Any], spec: str) -> dict[str, int]:
    return cfg.get("budget", {}).get("models", {}).get(spec, {})
