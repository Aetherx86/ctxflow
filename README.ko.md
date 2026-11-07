# ctxflow

여러 LLM을 쓰면서 토큰은 최소로 쓰는 도구입니다. 일상 작업은 코드와 저가 모델로 처리하고, 비용을 쓸 만한 판단에서만 상위 모델이나 여러 모델의 council을 부릅니다. 판단 로직은 WebAssembly 파일 하나에 들어 있어서 브라우저, Electron, 네이티브 바이너리에서 똑같이 동작합니다.

[앱 열기](https://aetherx86.github.io/ctxflow/) · [English](README.md)

## 동작 방식

앞 단계로 부족할 때만 다음 단계를 씁니다.

| 단계 | 담당 | 시점 | 토큰 |
|---|---|---|---|
| 0 | WASM 커널 | 항상 | 0 |
| 1 | 저가 모델, 읽기 전용 정찰 | 일상 작업 | 적음 |
| 2 | 어드바이저 한 명 | 계획 전, 같은 에러 2회, 완료 전 | 호출당 요약본 상한 1,500 |
| 3 | Council | 어드바이저가 판단이 팽팽하고 되돌리기 어렵다고 볼 때 | 실행 전에 최대치 표시 |

### 모드

`ctxflow mode` 또는 앱의 스위치로 고릅니다.

| 모드 | 동작 |
|---|---|
| Fast | 답변을 받아 확신도가 가장 높은 것을 고릅니다. 교차 검증은 없으며, 결과에 그렇게 표시합니다. |
| Auto (기본값) | 답이 일치하면 멈춥니다. 갈리면 채점하고, 승자가 분명하지 않을 때만 최종 모델을 부릅니다. |
| Thorough | 항상 채점하고 항상 최종 모델을 부릅니다. 토큰 한도는 그대로 적용됩니다. |

### Council

1. 각 모델이 서로의 답을 보지 않고 동시에 답합니다. 답은 약 400토큰 이내의 구조화된 카드입니다.
2. 서로 다른 모델이 같은 답을 내면 여기서 멈춥니다. 같은 모델의 복사본끼리 일치한 것은 합의로 치지 않습니다.
3. 각 모델이 다른 답을 익명으로 채점합니다. 모델명은 가려지고, 자기 답은 받지 않으며, 라벨은 평가자마다 섞입니다.
4. 코드가 점수를 매트릭스로 집계합니다. 평가자의 75% 이상이 같은 답을 1위로 꼽고 치명적 결함 보고가 없으면 여기서 끝납니다. 받은 답을 모두 채점한 평가자만 인정합니다.
5. 그렇지 않으면 최종 모델이 상위 두 답과 점수만 읽고 결정합니다.

모델에게 역할극을 시키지 않습니다. 각 좌석은 점검 항목(논리, 대안, 실패 모드, 실행 가능성)만 받습니다.

### 압축

매 턴 코드가 처리하므로 토큰이 들지 않습니다.

- 긴 코드와 로그는 `[#a1b2c3d4e5f6 code:py 120 lines; defs: login]` 같은 참조로 바뀌고, 원문은 해시로 보관됩니다. `ctxflow show <id>`로 바이트 단위까지 그대로 복원합니다.
- 같은 블록은 참조로 바뀌고, 반복 로그는 `(x200)`처럼 접힙니다.
- LLM 요약 후에는 경로, 함수명, 버전, 에러 문구가 남았는지 코드가 확인하고, 빠진 것을 다시 붙입니다.

어드바이저는 대화 대신 크기가 제한된 요약본을 읽습니다. 더 필요하면 보관된 원문을 id로 최대 2개까지 한 번만 요청할 수 있습니다.

## 설치

[Releases](https://github.com/Aetherx86/ctxflow/releases)에서 macOS, Linux, Windows용 바이너리를 받거나 직접 빌드합니다.

```bash
scripts/install.sh      # Rust 필요, ~/.local/bin에 설치
```

```bash
ctxflow init
ctxflow profile             # 이 컴퓨터에서 찾은 키와 모델, 프로파일 목록
ctxflow profile set local   # local, free, balanced, claude, api
```

키는 환경 변수로 넘깁니다. 로컬 모델은 키가 필요 없습니다.

| 모델 표기 | 사용처 | 필요한 것 |
|---|---|---|
| `ollama:auto`, `mlx:...`, `lmstudio:...` | 이 컴퓨터 | 없음 |
| `gemini:...` | Gemini API | `GEMINI_API_KEY` |
| `claude:haiku` | 로그인된 Claude Code | `claude` CLI |
| `anthropic:claude-opus-5-5` | Anthropic API | `ANTHROPIC_API_KEY` |
| `openrouter:...`, `openai:...`, `groq:...`, `deepseek:...` | 호스팅 서비스 | 해당 서비스의 키 |

OpenAI 호환 서비스는 `[providers.이름]`에 `base_url`과 `api_key_env`를 적어 추가합니다.

## 사용법

### 앱

```bash
ctxflow serve      # 이 컴퓨터의 키, 모델, Claude Code를 쓰는 앱을 엽니다
```

[호스팅된 앱](https://aetherx86.github.io/ctxflow/)은 정적 파일이라 직접 올려도 됩니다. 이 경우 키는 브라우저에 머물고 각 제공자로만 전송되며, Claude 모델은 쓸 수 없습니다. HTTPS에서는 PWA로 설치할 수 있습니다.

### CLI

```bash
ctxflow ask "이메일 검증 추가" -f src/login.ts
ctxflow council "SQLite와 Postgres 중 무엇을 쓸까?"
ctxflow council -o sqlite -o postgres "..."          # 선택지 고정
ctxflow advise plan "이 순서로 진행하려 합니다: ..."
make 2>&1 | ctxflow advise error --stdin "빌드 실패"
ctxflow advise done
ctxflow scout "토큰 갱신은 어디서 처리하나?"
ctxflow show a1b2c3d4e5f6
ctxflow status
```

`scout`는 Claude Code가 필요합니다. 답은 짧지만 탐색 과정에서 입력 토큰을 수만 개 읽을 수 있어 무료는 아닙니다.

### Claude Code mod

```bash
claude --plugin-dir mod --model haiku
```

상태 표시줄에 현재 상태를 보여 주고, Bash 명령이 실패하거나 턴이 끝날 때 `ctxflow advise`를 호출합니다. 게이트가 닫혀 있으면 모델 호출도, 컨텍스트 추가도 없습니다. `ctxflow init`을 하지 않은 폴더에서는 아무 일도 하지 않습니다.

### 스킬

mod를 쓸 수 없는 환경에서는 `skill/token-frugal-dev`가 에이전트에게 같은 규칙을 줍니다.

## 비용 제어

설정은 `.ctxflow/config.toml`의 `[council]`과 `[advisor]`, 또는 앱의 Spending limits에 있습니다.

- 실행 전에 최악의 경우를 표시하고, 기준을 넘으면 확인을 받습니다.
- 실행당 한도와 일일 한도는 각 단계 전에 검사합니다. 한도에 닿으면 그때까지의 최선을 "교차 검증되지 않음" 표시와 함께 돌려줍니다. 마지막 단계에서 조금 넘을 수 있습니다.
- 어드바이저는 세션당 3회, 6,000토큰으로 제한됩니다.
- 로컬 모델 사용량은 일일 한도에 포함되지 않습니다.

## 보안

- 클라우드로 가는 프롬프트에서 API 키, 토큰, 개인 키, 이메일, 카드 번호, 주민등록번호, SSN, 전화번호를 제거합니다. 규칙 기반이라 모든 형식을 잡지는 못합니다.
- 로컬 좌석은 클라우드 모델로 넘어가지 않습니다.
- 모델 응답은 텍스트 노드로만 그리며, 페이지에는 CSP가 적용됩니다.
- `ctxflow serve`는 루프백에만 바인딩하고 시작 URL의 토큰, Host, Origin을 검사합니다.
- `.ctxflow/archive`에는 원문이 그대로 저장되며, 나중에 프롬프트에서 제거되는 내용도 포함됩니다. 프로젝트의 git에서는 제외됩니다.

### 난독화의 한계

커널은 프롬프트를 XOR로 인코딩하고, JS 호스트는 [sentinel-kit](https://github.com/Aetherx86/sentinel-kit)의 난독화기로 빌드합니다. 빌드된 파일을 베끼기 어렵게 할 뿐입니다. 소스는 공개되어 있고, 프롬프트는 각 제공자에게 평문으로 전달됩니다.

## 검증 상태

| 경로 | 확인 방법 |
|---|---|
| Ollama, Claude Code CLI | 실제 호출 |
| Gemini, Anthropic API, OpenAI 호환 서비스 | 요청과 응답 형식을 확인하는 로컬 테스트 서버. 실제 호출은 아님 |

## 구조

```
kernel/      Rust, 의존성 없음. JSON 입출력만 하고 I/O는 하지 않음. WebAssembly로 빌드
cli/         네이티브 바이너리: CLI, 프로바이더, 앱의 로컬 서버
hosts/js/    브라우저와 Electron에서 커널을 구동하는 얇은 호스트
hosts/web/   앱
mod/         Claude Code mod
skill/       mod를 불러올 수 없는 도구를 위한 스킬
```

커널은 입출력을 하지 않는 상태 머신입니다. 호스트는 커널이 요청한 호출을 실행해 결과를 돌려주기만 하므로, 새 플랫폼은 호스트만 작성하면 됩니다.

## 빌드와 테스트

```bash
scripts/build.sh                                  # 커널, JS 호스트, 앱, 바이너리
cargo test --workspace
cd hosts/js && node --no-warnings --test test/    # 번들 검증은 CTXFLOW_HOST=../dist/index.mjs 추가
```

JS 호스트를 다시 빌드하려면 sentinel-kit의 `tools/obfuscate.mjs`가 필요합니다. `~/Documents`에 없으면 `SENTINEL_KIT`로 경로를 지정합니다.

## 참고한 프로젝트

독립 답변, 익명 평가, 최종 합성으로 이어지는 council 구조는 [karpathy/llm-council](https://github.com/karpathy/llm-council)의 아이디어입니다. 라이선스가 없는 저장소라 코드는 가져오지 않았습니다.

## 라이선스

MIT
