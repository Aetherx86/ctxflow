# Changelog

## [0.4.0] - 2026-11-21

Added

- Council scoring weights. `[council.weights]` in `.ctxflow/config.toml` (or `limits.weights` in the JS host) sets how much evidence, risk and feasibility count in each reviewer's score. Each weight defaults to 1 and is capped at 100; negative or non-numeric weights count as 1, and all zero falls back to equal weights.
- Pairwise results. The council result now has `agreement`, the share of head-to-head comparisons where the reviewers agree with the majority, `condorcet_winner`, an answer that beats every other answer head to head (or `null`), `complete_reviewers` and the `weights` used. Matrix rows add `weighted`, `spread` (highest minus lowest weighted score from different reviewers), `votes`, `first_share` and `pairwise_wins`. These are reported only; the decision rules of the dial are unchanged. The CLI table shows the weighted score and pairwise wins, and the app shows agreement and the pairwise winner.
- `ctxflow council --estimate [--json]` and the kernel op `preflight`: the worst-case tokens of each stage, the number of distinct models among the seats, and how many secrets and hidden characters would be removed from the question. No model is called. In Fast mode only the first stage counts. The JS host exports `preflightCouncil`.
- `ctxflow expand` and the kernel op `expand`: replace `[#id ...]` and `[same as #id]` references with the stored originals, including references inside originals up to `max_depth` levels (default 4, at most 8; the CLI uses 8), with cycle detection and caps on output size (16 MB) and the number of references expanded (100,000). For a log reference, the sample lines that compression kept after it are dropped, so the original log comes back as it was. `--only ID` expands selected references. The JS host exports `expandRefs`.
- The kernel op `unredact` and the JS export `unredactText` put redacted values back from a redaction map, in one pass, so a restored value can never be expanded again and `[REDACTED_1]` never matches inside `[REDACTED_10]`.
- `ctxflow stats [--days N] [--here] [--json]`: calls, tokens and cost from the usage ledger, by model, by kind and by UTC day, with cloud and local use apart. Costs a provider reported are kept apart from estimates made with optional `[pricing."backend:model"]` rates (`input_per_mtok`, `output_per_mtok`). Unreadable ledger lines are counted and skipped.
- `verify_summary` also returns `retained`, `total` and `ratio` for the identifiers kept.

Changed

- Redaction replaces every `NAME=value` and every `Bearer` token on a line, not only the first one. It also covers `ACCESS_KEY`, `SIGNING_KEY`, `CREDENTIAL` and `PRIVATE-KEY` names and the `pk_live_`, `whsec_`, `glrt-`, `xoxr-`, `xoxe-`, `sq0atp-`, `SG.` and `sbp_` prefixes. One value gets one placeholder everywhere it appears.
- Log detection covers Rust panics and backtraces, Go goroutine dumps and `--- FAIL:` lines, `Caused by:` chains, and the `FATAL`, `CRITICAL` and `CRIT` levels.
- Answers with equal scores from a reviewer now share the rank points for their places, instead of the sort order deciding. Equal Borda scores are ordered by weighted score, then by fewer reported flaws, then by confidence.
- The JS host runs the `claude` CLI with `--setting-sources user`, as the native binary already did.
- The kernel binary is 171 KB, up from 145 KB. Its exports (`memory`, `alloc`, `call`, `reset`) are unchanged.

Fixed

- A `\uD83D`-style high surrogate escape at the end of a JSON string made the whole input invalid, and one followed by an escape that is not a low surrogate swallowed that character. Both now give U+FFFD and keep the next character.
- A crafted council state with a card index out of range or repeated, a seat number of 0 or no cards could stop the kernel. Such entries are skipped, and an empty card list ends with `failed`.
- A council with more than 26 seats is refused, as reviewers label answers A to Z.
- Negative or non-finite `code_max`, `log_max` and `max` values in kernel requests fall back to their defaults, and huge ones are capped.
- The status line percentage cannot overflow.

## 0.3.0

Changed

- The host is now one Rust binary with the kernel linked in, for macOS, Linux and Windows. It needs no Python or `wasmtime`. Project files, sessions and the usage ledger keep their format, so existing projects continue to work.
- `ctxflow advise` reads stdin only if data arrives within 0.2 s, so an open and silent stdin cannot hang it. `--stdin` waits for the end of input, for slow commands such as a build.
- The kernel binary is now 145 KB, down from 170 KB, and no longer carries internal function names.
- Config comments, profile descriptions and document templates are in English.
- The mod's error hook passes `--stdin`, so it always reads the whole output.

Security

- A project's `.ctxflow/config.toml` is untrusted until `ctxflow trust`. Until then, settings that could run a program, send a key or a prompt somewhere else, read files outside the project, or raise spending limits are ignored. Trust is recorded for the exact file, so an edit needs trust again. `ctxflow init` trusts the file it writes, and `ctxflow trust --revoke` withdraws trust.
- `ctxflow advise --auto`, used by the mod, stays silent in a project whose config is not trusted. Status lines show `not trusted`.
- State files, archives and backups must be plain paths inside the project. Symbolic links are not followed, and files are written as new files with mode 0600.
- Git runs with fsmonitor, external diff and textconv turned off, and only in trusted projects. Claude Code runs with `--setting-sources user`, so a repository's own settings and hooks do not load.
- Hidden characters (bidirectional controls, tag characters, variation selectors and zero-width characters) are removed from prompts, replies and terminal output. New kernel op: `sanitize`.
- Council answers are kept to one line per field, so an answer cannot fake the labels that reviewers read. Prompts treat quoted content and other models' answers as data, and an answer that tries to influence its own score counts as a fatal flaw.
- A model name that starts with `-` or contains whitespace is refused, so it cannot turn into a command-line option.
- `ctxflow serve`: the start-up URL's token works once and is exchanged for a session token. Tokens are compared in constant time. At most 64 connections, and 20 seconds per request.
- Redaction covers more key formats and passwords in URLs.
- Compression no longer slows down sharply on hostile input with many repeated or distinct lines.
- Error messages are sanitized before they reach the terminal.
- A fuzz test of the kernel's JSON interface runs with the test suite.

Added

- Anthropic API requests use API version `2026-10-21`, and `claude-opus-5-5`, `claude-opus-5`, `claude-fable-5-1` and `claude-sonnet-5-5` opt into the server-side refusal fallback.
- `ctxflow scout` is now recorded in the usage ledger.
- A release workflow that attaches binaries and the web app.
- `README.ko.md`.

Fixed

- Floats in kernel JSON were cut to three decimals, which rounded small costs in the ledger to zero.
- A number such as `1e999` from a model produced invalid JSON.
- `api` profile model ids were checked against OpenRouter, and one that does not exist was replaced.
- The browser no longer offers an `anthropic:` path it could not run.

Removed

- The Python host.

## 0.2.0

Added

- `kernel/`: a dependency-free Rust kernel compiled to one WebAssembly binary. Every token-free decision lives there, so Python, Node and browsers behave identically.
- Reversible compression: long code and logs become `[#id ...]` references, and `ctxflow show <id>` returns the original bytes.
- Pinned references: identifiers a model drops from a summary are put back by code.
- `ctxflow council`: independent answers run in parallel, scoring is anonymous, results form a matrix, and one model decides only if needed.
- `ctxflow advise`: a stronger model behind a gate. It reads a capped brief and can pull at most two stored originals once.
- `ctxflow scout`: a cheap read-only search that returns a summary.
- `ctxflow mode fast|auto|thorough`: one dial for how far escalation goes.
- `ctxflow profile`: choose providers. Includes local-only, Gemini free tier, mixed, Claude Code and own-key setups.
- Own-key and local backends: any OpenAI-compatible service, Anthropic, Ollama, MLX and LM Studio.
- Removal of credentials and personal data before a prompt leaves the machine. Local models are exempt.
- A web app that runs anywhere as static files, and `ctxflow serve` to use this machine's keys and the Claude Code CLI.
- A Claude Code mod with a status line and gated advice.
- Spending limits per run and per day, and the maximum cost is shown before a run.

Changed

- Pruning replaced "elided" markers, which lost the original, with stored references.
- `wasmtime` is now a dependency, to run the kernel.

Fixed

- `~/.ctxflow`, which holds the global ledger, was mistaken for a project.

## 0.1.0

- First version: stateless prompt assembly, rolling summaries, handoffs and model routing.
