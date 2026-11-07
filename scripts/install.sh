#!/usr/bin/env bash
# Install the ctxflow CLI and link the Antigravity skill into the global skills dir.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
SKILLS="${HOME}/.gemini/config/skills"

uv tool install -e "$HERE" --force

mkdir -p "$SKILLS"
ln -sfn "$HERE/skill/token-frugal-dev" "$SKILLS/token-frugal-dev"
echo "linked skill → $SKILLS/token-frugal-dev"
echo "uninstall: uv tool uninstall ctxflow && rm $SKILLS/token-frugal-dev"

