# Changelog

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
