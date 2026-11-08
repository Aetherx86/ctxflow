# Changelog

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
