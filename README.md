# ctxflow

Use several LLMs while spending as few tokens as possible. Routine work runs in code and on cheap models, and a stronger model or a council of models is called only when a decision is worth the cost. The decision logic is one WebAssembly file, so it behaves the same in a browser, in Electron and in the native binary.

[Open the app](https://aetherx86.github.io/ctxflow/) · [한국어](README.ko.md)

## How it works

Each step is used only when the one before it is not enough.

| Step | Who | When | Tokens |
|---|---|---|---|
| 0 | WASM kernel | always | 0 |
| 1 | Cheap model, read-only scout | routine work | few |
| 2 | One advisor | before a plan, after the same error twice, before done | a brief capped at 1,500 per call |
| 3 | Council | when the advisor finds the choice balanced and costly to reverse | shown before it runs |

### Modes

Set with `ctxflow mode` or the switch in the app.

| Mode | What it does |
|---|---|
| Fast | Collects answers and takes the most confident one. Nothing is cross-checked, and the result says so. |
| Auto (default) | Stops if the answers agree. If not, they are scored, and the final model runs only when no answer clearly wins. |
| Thorough | Always scores and always runs the final model. Token limits still apply. |

### Council

1. Each model answers at the same time without seeing the others. An answer is a structured card of at most about 400 tokens.
2. If different models give the same answer, it stops here. Copies of one model agreeing does not count as agreement.
3. Each model scores the other answers anonymously: model names are hidden, it never gets its own answer, and labels are shuffled per reviewer.
4. Code combines the scores into a matrix. If at least 75% of the reviewers rank one answer first and no fatal flaw is reported, it stops here. A reviewer counts only if it scored every answer it was shown.
5. Otherwise a final model reads the top two answers and the scores, and decides.

Each reviewer's score for an answer is the weighted mean of its evidence, risk and feasibility scores. The weights are 1 unless `[council.weights]` sets them (see [Estimates, statistics and expansion](#estimates-statistics-and-expansion)). Answers a reviewer scores equally share the rank points for their places.

No model is asked to play a role. Each seat gets a list of things to check: logic, alternatives, failure modes and feasibility.

### Compression

This runs in code on every turn and costs no tokens.

- Long code and logs become references such as `[#a1b2c3d4e5f6 code:py 120 lines; defs: login]`. The original is stored by hash, and `ctxflow show <id>` returns it byte for byte.
- Identical blocks become references, and repeated log lines collapse to `(x200)`.
- After an LLM summary, code checks that paths, function names, versions and error text survived, and puts back any that did not.

The advisor reads a capped brief instead of the conversation. If it needs more, it can ask for up to two stored originals by id, once.

## Install

Download a binary from [Releases](https://github.com/Aetherx86/ctxflow/releases) for macOS, Linux or Windows, or build it:

```bash
scripts/install.sh      # needs Rust; installs to ~/.local/bin
```

```bash
ctxflow init
ctxflow profile             # keys and models found on this machine, and the profiles
ctxflow profile set local   # local, free, balanced, claude, api
```

Keys come from environment variables. Local models need none.

| Model | Runs on | Needs |
|---|---|---|
| `ollama:auto`, `mlx:...`, `lmstudio:...` | this machine | nothing |
| `gemini:...` | Gemini API | `GEMINI_API_KEY` |
| `claude:haiku` | signed-in Claude Code | the `claude` CLI |
| `anthropic:claude-opus-5-5` | Anthropic API | `ANTHROPIC_API_KEY` |
| `openrouter:...`, `openai:...`, `groq:...`, `deepseek:...` | hosted services | that service's key |

Add any OpenAI-compatible service under `[providers.name]` with `base_url` and `api_key_env`.

## Use

### App

```bash
ctxflow serve      # opens the app with this machine's keys, models and Claude Code
```

The [hosted app](https://aetherx86.github.io/ctxflow/) is static files that you can also host yourself. There, keys stay in the browser and go only to each provider, and Claude models are not available. Over HTTPS it installs as a PWA.

### CLI

```bash
ctxflow ask "add email validation" -f src/login.ts
ctxflow council "SQLite or Postgres?"
ctxflow council -o sqlite -o postgres "..."          # fixed choices
ctxflow advise plan "I will do this in this order: ..."
make 2>&1 | ctxflow advise error --stdin "build failed"
ctxflow advise done
ctxflow scout "where are tokens refreshed?"
ctxflow show a1b2c3d4e5f6
ctxflow expand -f notes.md                           # put stored originals back into text
ctxflow council --estimate "SQLite or Postgres?"     # cost per stage, no model calls
ctxflow stats --days 7                               # calls, tokens and cost from the ledger
ctxflow status
```

`scout` needs Claude Code. Its answer is short, but the search itself can read tens of thousands of input tokens, so it is not free.

A `.ctxflow/config.toml` that came with a repository you cloned is not trusted. Read it, then run `ctxflow trust` (see [Security](#security)). `ctxflow trust --revoke` withdraws trust.

### Claude Code mod

```bash
claude --plugin-dir mod --model haiku
```

It shows state in the status line and calls `ctxflow advise` when a Bash command fails and when a turn ends. If the gate stays closed, there is no model call and nothing is added to the context. In a folder without `ctxflow init`, or one whose config is not trusted, it does nothing.

### Skill

Where a mod cannot run, `skill/token-frugal-dev` gives an agent the same rules.

## Estimates, statistics and expansion

Added in 0.4.0. None of these commands calls a model.

### What a council would cost

```bash
ctxflow council --estimate "SQLite or Postgres for a single-user desktop app?"
```

```
mode auto · 3 seat(s), 1 distinct model(s)
  answers   ~2046 tokens
  scoring   ~3780 tokens
  decision  ~1709 tokens
  at most   ~7535 tokens · limit 20000
question ~13 tokens, ~13 after compression
── one model only, so agreement would not be independent
```

This was the `local` profile, whose three seats are the same local model. `--json` prints the same as one object (`stage1`, `stage2`, `stage3`, `worst_case`, `distinct_models`, `independent`, `redactions`, `hidden_removed`, `weights`). In Fast mode only the first stage counts.

### Scoring weights

```toml
[council.weights]
evidence = 1.0
risk = 3.0          # weigh risks three times as much as the other two
feasibility = 1.0
```

Weights default to 1 and are capped at 100. The result of `ctxflow council --json` reports the weights used, each answer's `weighted` score, `spread` (how far reviewers disagreed on it) and `pairwise_wins`, and for the whole run `agreement` (the share of head-to-head comparisons where reviewers agree with the majority) and `condorcet_winner` (an answer that beats every other answer head to head, or `null`). These figures are reported only. When to stop and when to call the final model still follows the [modes](#modes).

### Usage statistics

```bash
ctxflow stats                       # all projects, all time
ctxflow stats --days 7 --here       # this project, last 7 days
ctxflow stats --json
```

It reads the usage ledger in `~/.ctxflow/usage.jsonl` and totals calls, tokens and cost by model, by kind (`ask/light`, `council/review`, `scout`, ...) and by UTC day, with local models apart from cloud ones. Costs a provider reported are kept apart from estimates. For a provider that reports no cost, add rates in US dollars per million tokens:

```toml
[pricing."gemini:gemini-3.5-flash"]
input_per_mtok = 0.15
output_per_mtok = 0.60
```

These rates are an example: check the provider's current prices.

### Putting originals back

Compression replaces long code and logs with references such as `[#44542d176dc0 code:py 40 lines]`. `ctxflow expand` replaces them with the originals from `.ctxflow/archive`, from text given as an argument, with `-f FILE` or on stdin, and writes the result to stdout:

```bash
ctxflow expand -f reply.md > reply.full.md
ctxflow expand --only 44542d176dc0 < reply.md
ctxflow expand --json "see [#deadbeef0000 code 3 lines]"   # reports ids not in the archive
```

References inside originals are expanded too, up to 8 levels, and a reference back to an original being expanded is left as it is. For a log, the sample lines that compression kept after the reference are dropped, so the log comes back as it was. Output stops at 16 MB.

The same operations are kernel ops (`preflight`, `expand`, `unredact`) and JS host exports (`preflightCouncil`, `expandRefs`, `unredactText`).

## Controlling cost

Settings are under `[council]` and `[advisor]` in `.ctxflow/config.toml`, or Spending limits in the app.

- The worst case is shown before a run, with a confirmation above a threshold.
- Limits per run and per day are checked before each stage. At a limit, you get the best answer so far, marked as not cross-checked. The last stage can go slightly over.
- The advisor is limited to 3 calls and 6,000 tokens per session.
- Local models do not count against the daily limit.

## Security

- Prompts bound for a cloud model have API keys, tokens, private keys, passwords in URLs, email addresses, card numbers, Korean resident numbers, SSNs and phone numbers removed. This is rule-based and will not catch every format.
- A local seat never falls back to a cloud model.
- A project's `.ctxflow/config.toml` may come from a stranger. Until you run `ctxflow trust`, settings that could run a program, send a key or a prompt somewhere else, read files outside the project, or raise spending limits are ignored, and the mod's hooks stay silent. Trust covers the exact file, so any later edit needs `ctxflow trust` again. `ctxflow init` trusts the file it writes.
- Files that ctxflow reads or writes must be plain paths inside the project. It does not follow symbolic links, and it writes new files instead of opening existing ones.
- Git runs with fsmonitor, external diff tools and textconv filters turned off, and Claude Code runs with your user settings only, so a repository cannot run its own programs through either.
- Hidden characters are removed from prompts, replies and terminal output: bidirectional controls, tag characters, variation selectors and zero-width characters.
- Prompts mark quoted files, logs and other models' answers as data, not instructions, and each field of a council answer is kept to one line, so an answer cannot fake the labels that reviewers read. This lowers the risk of prompt injection but does not remove it.
- A model name cannot start with `-`, so it cannot turn into a command-line option.
- Model replies are drawn as text nodes only, and the page has a Content Security Policy.
- `ctxflow serve` binds to loopback only. The token in its start-up URL works once: the page exchanges it for a session token, so a copied URL is useless afterwards. It also checks the Host and Origin headers, and limits connections to 64 and each request to 20 seconds.
- `.ctxflow/archive` stores originals as they were, including anything later removed from prompts. It is excluded from the project's git.
- The tests include fuzzing of the kernel's JSON interface, and the local server has been fuzzed with malformed requests.

### What obfuscation does not do

The kernel XOR-encodes its prompts, and the JS host is built with the obfuscator from [sentinel-kit](https://github.com/Aetherx86/sentinel-kit). This slows down copying of the built files and nothing more. The source is open, and prompts reach each provider in plain text.

## Status

| Path | Checked with |
|---|---|
| Ollama, Claude Code CLI | real calls |
| Gemini, Anthropic API, OpenAI-compatible services | a local test server that checks request and response format, not real calls |

## Layout

```
kernel/      Rust with no dependencies. JSON in and out, no I/O. Builds to WebAssembly.
cli/         the native binary: CLI, providers and the app's local server
hosts/js/    thin host that drives the kernel in browsers and Electron
hosts/web/   the app
mod/         Claude Code mod
skill/       skill for tools that cannot load the mod
```

The kernel is a state machine that performs no I/O. A host only makes the calls the kernel asks for and returns the results, so a new platform needs only a new host.

## Build and test

```bash
scripts/build.sh                                  # kernel, JS host, app and binary
cargo test --workspace
cd hosts/js && node --no-warnings --test test/    # add CTXFLOW_HOST=../dist/index.mjs to test the bundle
```

Rebuilding the JS host needs `tools/obfuscate.mjs` from sentinel-kit. Point `SENTINEL_KIT` at a checkout if it is not in `~/Documents`.

## Credits

The council's structure of independent answers, anonymous review and a final synthesis comes from [karpathy/llm-council](https://github.com/karpathy/llm-council). That repository has no license, so only the idea is used here, not its code.

## License

MIT
