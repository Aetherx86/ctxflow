---
name: token-frugal-dev
description: >-
  Token-frugal, stateless development workflow. Use when the user wants to save tokens or stay
  within free-tier limits, asks for context pruning, a rolling summary, a handoff to a new
  session/chat/app, or progress tracking in PROGRESS.md/SPEC.md/HANDOFF.md, or says "continue
  from HANDOFF.md". Also covers offloading light work to cheap models (flash subagents, or the
  `ctxflow` CLI routing to Gemini Flash-Lite / Claude Code).
---

# Token-Frugal Dev Workflow

Principle: **the model has no memory; the documents do.** Conversation history is disposable,
PROGRESS.md is the single source of truth. Every step should be continuable by a fresh session
that reads only `SPEC.md` + `PROGRESS.md` (+ `HANDOFF.md`) + the 1 file being changed.

## 1. Bootstrap (once per project)

1. Look for `SPEC.md` and `PROGRESS.md` at the project root.
2. If missing, run `ctxflow init` (creates both plus `.ctxflow/config.toml`). If the `ctxflow`
   command is unavailable, copy the templates in [resources/](resources/) instead.
3. Fill SPEC.md from what the user said (goal, constraints, stack, key decisions) in ≤ 30 lines.

## 2. Resume (start of every new conversation)

If the user says "continue", "이어서", or mentions HANDOFF.md:
read **only** `HANDOFF.md`, `PROGRESS.md`, `SPEC.md`, then the single file named in "next step".
Do not scan the repo broadly; do not ask the user to re-explain.

## 3. While working: context pruning rules

- Read files by line range around the target symbol; never re-read a file already in context
  unless it changed.
- Command output: always trim (`| tail -n 40`, `grep -n`, `--quiet`). Never pull full logs or
  full test output into context; extract the first error + its location.
- Once an error is fixed, never quote its log again; refer to it in one line.
- Show the user diffs / changed functions only, not whole files.
- One code file in focus at a time; state docs carry the rest.

## 4. Update state docs (after each meaningful step)

After a feature/fix lands or a decision is made, edit PROGRESS.md directly:
move items to "완료/Done", update "현재 상태", rewrite "다음 할 일" (numbered, first item = the
exact next action). Keep PROGRESS.md < 60 lines; collapse old Done items into one line each.
Record new irreversible decisions in SPEC.md ("핵심 설계 결정").

## 5. Model routing (spend expensive tokens only on hard thinking)

| Work | Route |
|---|---|
| Design, root-cause debugging, security/perf review | Do it yourself (main model) |
| Summaries, boilerplate, renames, docstrings, commit messages, simple greps | `invoke_subagent` with `Model: "flash_lite"` or `"flash"` |
| Second opinion / offload to user's free Gemini quota or Claude Code | `ctxflow ask -t light "..." -f <file>` or `ctxflow ask -t heavy "..." -f <file>` |

`ctxflow ask` assembles SPEC + PROGRESS + rolling summary + last turns + one file itself and
prints token usage to stderr. Check remaining quota with `ctxflow status` before heavy batches.

## 6. Handoff (roll the session over)

Trigger when any holds: the conversation has had roughly 25+ tool calls, a milestone is done,
context feels repetitive, or the user asks.

1. Update PROGRESS.md (step 4).
2. Write `HANDOFF.md` using [resources/HANDOFF.template.md](resources/HANDOFF.template.md):
   ≤ 10 bullets — goal, done, current blocker, **exact next step**, files involved.
   (If the user is using ctxflow sessions in the terminal, `ctxflow handoff` does steps 1-2.)
3. Tell the user: start a new conversation and type `HANDOFF.md 이어서` — or use `/compact`
   to stay in this one. For another app/platform, paste HANDOFF.md + SPEC.md + PROGRESS.md +
   the one file being changed.

## Verify

- `PROGRESS.md` "다음 할 일" item 1 is concrete enough to act on without this conversation.
- `HANDOFF.md` ≤ 10 bullets and names real file paths.
- `ctxflow status` (if used) shows no model at ≥ 90% of its configured limit.

## Escalation

Spend in this order. Each step is used only when the one before it is not enough.

1. Decide in code when you can. Compress with `ctxflow`, and restore originals with `ctxflow show <id>` instead of asking for files again.
2. Search with `ctxflow scout "..."`. You get a summary with file:line references, not the search output.
3. Ask the stronger model at checkpoints: `ctxflow advise plan "..."` before a plan, `ctxflow advise error "..."` with the log on stdin after a failure, and `ctxflow advise done` before finishing. Its gate stays silent unless the call is worth paying for.
4. For a costly choice that is hard to reverse, run `ctxflow council "..."`. It states the maximum cost first.

`ctxflow mode fast|auto|thorough` sets how far this goes. Respect the user's choice of mode.
