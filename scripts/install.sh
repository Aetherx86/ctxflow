#!/usr/bin/env bash
# Installs the ctxflow binary from source and links the skill for Antigravity.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
SKILLS="${HOME}/.gemini/config/skills"

cargo install --path "$HERE/cli" --root "${HOME}/.local" --locked --force

mkdir -p "$SKILLS"
ln -sfn "$HERE/skill/token-frugal-dev" "$SKILLS/token-frugal-dev"
echo "installed: ${HOME}/.local/bin/ctxflow"
echo "linked skill: $SKILLS/token-frugal-dev"
echo "uninstall: cargo uninstall ctxflow --root ${HOME}/.local, then delete the skill link above"
