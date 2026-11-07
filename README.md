# ctxflow

여러 모델의 판단을 쓰면서 토큰을 최소로 쓰는 도구입니다. 평소에는 코드와 저가 모델로 처리하고, 판단이 갈릴 때만 비싼 모델을 호출합니다. 판단 로직은 WebAssembly 파일 하나에 들어 있어서 브라우저, Node, Python에서 똑같이 동작합니다.

## 동작 방식

비용이 낮은 단계부터 시도하고, 필요할 때만 다음 단계로 넘어갑니다.

| 단계 | 담당 | 시점 | 토큰 |
|---|---|---|---|
| 0 | WASM 커널 | 항상 | 0 |
| 1 | 저가 실행 모델, 읽기 전용 정찰 | 일상 작업 | 적음 |
| 2 | 어드바이저 한 명 | 계획 전, 같은 에러 2회, 완료 전 | 호출당 요약본 상한 1,500 |
| 3 | Council | 어드바이저도 판단이 갈릴 때 | 실행 전 최대치 표시 |

### 모드

`ctxflow mode` 또는 앱의 스위치로 고릅니다.

| 모드 | 동작 |
|---|---|
| Fast | 답변만 받아 가장 확신도 높은 것을 고릅니다. 교차 검증은 없고, 결과에 그렇게 표시합니다. |
| Auto (기본값) | 답이 일치하면 멈춥니다. 갈리면 채점하고, 승자가 불분명할 때만 최종 모델을 부릅니다. |
| Thorough | 항상 채점하고 항상 최종 모델을 부릅니다. 토큰 한도는 그대로 적용됩니다. |

### Council

1. 각 모델이 서로의 답을 보지 않고 동시에 답합니다. 답은 400토큰 이내의 구조화된 카드입니다.
2. 서로 다른 모델이 같은 답을 내면 여기서 멈춥니다. 같은 모델의 복사본끼리 일치한 것은 합의로 치지 않습니다.
3. 각 모델이 다른 답을 익명으로 채점합니다. 모델명은 가려지고, 자기 답은 받지 않으며, 라벨은 평가자마다 섞입니다.
4. 코드가 점수를 매트릭스로 집계합니다. 평가자의 75% 이상이 같은 답을 1위로 꼽고 치명적 결함 보고가 없으면 여기서 끝납니다.
5. 그렇지 않을 때만 최종 모델이 상위 두 답과 점수만 읽고 결정합니다.

모델에게 역할극을 시키지 않습니다. 각 좌석은 점검 항목(논리, 대안, 실패 모드, 실행 가능성)만 받습니다.

### 압축

매 턴 코드가 처리하므로 토큰이 들지 않습니다.

- 긴 코드와 로그는 `[#a1b2c3d4e5f6 code:py 120 lines; defs: login]`로 바뀌고, 원문은 해시 주소로 보관됩니다. `ctxflow show <id>`로 바이트 단위까지 그대로 복원합니다.
- 같은 블록은 참조로 바뀌고, 반복 로그는 `(x200)`처럼 접힙니다.
- LLM 요약 후에는 경로, 함수명, 버전, 에러 문구가 빠졌는지 코드가 확인하고, 빠진 항목을 다시 붙입니다.

어드바이저는 전체 대화 대신 크기가 제한된 요약본을 읽습니다. 더 필요하면 `NEED: #id`로 원문을 최대 2개까지 한 번만 요청할 수 있습니다.

## 설치

```bash
uv tool install -e .
ctxflow init
ctxflow profile                # 감지된 키와 모델, 프로파일 목록
ctxflow profile set local      # local, free, balanced, claude, api
```

키는 환경 변수로 넘깁니다. 로컬 모델은 키가 필요 없습니다.

| 모델 표기 | 사용처 | 필요한 것 |
|---|---|---|
| `ollama:auto`, `mlx:...`, `lmstudio:...` | 이 컴퓨터의 모델 | 없음 |
| `gemini:...` | Gemini API | `GEMINI_API_KEY` |
| `claude:haiku` | 로그인된 Claude Code | `claude` CLI |
| `anthropic:claude-opus-5-5` | Anthropic API | `ANTHROPIC_API_KEY`와 `--with anthropic` 설치 |
| `openrouter:...`, `openai:...`, `groq:...`, `deepseek:...` | 호스팅 서비스 | 각 서비스의 키 |

OpenAI 호환 서비스는 설정의 `[providers.이름]`에 `base_url`과 `api_key_env`를 적어 추가합니다.

## 사용법

### 앱

```bash
ctxflow serve        # 이 컴퓨터의 키와 Claude Code를 쓰는 앱을 엽니다
```

`src/ctxflow/web`은 정적 파일이므로 아무 곳에나 올려도 됩니다. 이 경우 키는 브라우저에 머물고 각 제공자로만 전송되며, Claude Code와 Anthropic 모델은 쓸 수 없습니다. HTTPS로 배포하면 PWA로 설치할 수 있습니다.

### CLI

```bash
ctxflow ask "이메일 검증 추가" -f src/login.ts
ctxflow council "SQLite와 Postgres 중 무엇을 쓸까?"
ctxflow council -o sqlite -o postgres "..."   # 선택지 고정
ctxflow advise plan "이 순서로 진행하려 합니다: ..."
make 2>&1 | ctxflow advise error "빌드 실패"
ctxflow advise done
ctxflow scout "토큰 갱신은 어디서 처리하나?"
ctxflow show a1b2c3d4e5f6
ctxflow status
```

### Claude Code mod

`mod/`를 플러그인으로 불러옵니다.

```bash
claude --plugin-dir mod --model haiku
```

상태 표시줄에 현재 상태를 보여 주고, Bash가 실패하거나 턴이 끝나면 `ctxflow advise`를 호출합니다. 게이트가 닫혀 있으면 모델 호출도, 컨텍스트 추가도 없습니다. `ctxflow init`을 하지 않은 폴더에서는 아무 일도 하지 않습니다.

## 비용 제어

설정은 `.ctxflow/config.toml`의 `[council]`과 `[advisor]`, 또는 앱의 Spending limits에 있습니다.

- 실행 전에 최악의 경우 토큰을 표시하고, 기준을 넘으면 확인을 받습니다.
- 실행당 한도와 일일 한도는 각 단계 전에 검사합니다. 한도에 닿으면 그때까지의 최선을 "교차 검증되지 않음"이라는 표시와 함께 돌려줍니다.
- 어드바이저는 세션당 3회, 6,000토큰으로 제한됩니다.
- 로컬 모델 사용량은 일일 한도에 포함되지 않습니다.

## 보안

- 클라우드로 보내는 프롬프트에서 API 키, 토큰, 개인 키, 이메일, 카드 번호, 주민등록번호, SSN, 전화번호를 제거합니다. 제거는 정규식이 아닌 규칙 기반이며 모든 형식을 잡는다고 보장하지 않습니다.
- 로컬 좌석은 클라우드 모델로 넘어가지 않습니다.
- 모델 응답은 텍스트 노드로만 그립니다. 페이지에는 CSP가 적용됩니다.
- `ctxflow serve`는 루프백에만 바인딩하고 시작 URL의 토큰, Host, Origin을 검사합니다.
- `.ctxflow/archive`에는 프롬프트에서 제거된 원문이 그대로 저장됩니다. 저장소에 커밋되지 않도록 제외되어 있습니다.

### 난독화의 한계

커널의 프롬프트는 XOR로 인코딩되어 있고 JS 호스트는 sentinel-kit의 난독화기로 빌드합니다. 코드를 그대로 베끼기 어렵게 할 뿐, 숨기지는 못합니다. 프롬프트는 제공자에게 평문으로 전송되므로 자기 키를 쓰는 사용자는 프록시로 볼 수 있습니다.

## 구조

```
kernel/        Rust. 의존성 없음. JSON 입출력만 하고 I/O는 하지 않음
hosts/js/      커널을 구동하는 얇은 호스트 (Node, Electron, 브라우저)
hosts/web/     앱
src/ctxflow/   Python 호스트와 CLI, serve
mod/           Claude Code mod
skill/         mod를 쓸 수 없는 환경을 위한 스킬
```

커널은 입출력을 하지 않는 상태 머신입니다. 호스트는 커널이 요청한 호출을 실행해 결과를 돌려주기만 하므로, 새 플랫폼은 호스트만 작성하면 됩니다.

## 빌드와 테스트

```bash
scripts/build.sh                                  # 커널, JS 호스트, 앱
cd kernel && cargo test
python -m unittest discover -s tests
cd hosts/js && node --no-warnings --test test/    # CTXFLOW_HOST=../dist/index.mjs 로 번들 검증
```

JS 호스트 빌드에는 sentinel-kit의 `tools/obfuscate.mjs`가 필요합니다(`SENTINEL_KIT` 환경 변수로 경로 지정). 이 도구는 템플릿 리터럴 안의 변수 이름을 바꾸지 않으므로, 호스트 코드에서는 지역 변수를 `${}`로 넣지 않습니다.

## 참고한 프로젝트

Council의 독립 답변, 익명 평가, 최종 합성 구조는 [karpathy/llm-council](https://github.com/karpathy/llm-council)의 아이디어입니다. 라이선스가 명시되지 않은 저장소라 코드는 가져오지 않았습니다.
