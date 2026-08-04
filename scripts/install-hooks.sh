#!/usr/bin/env bash
# Point git at the tracked hooks in .githooks/ (git hooks live in .git/hooks,
# which isn't version-controlled — core.hooksPath is how a repo ships them).
#
# Run once per clone:  scripts/install-hooks.sh
# Undo:                git config --unset core.hooksPath
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
chmod +x .githooks/* 2>/dev/null || true

echo "hooks installed (core.hooksPath = .githooks):"
for h in .githooks/*; do
  [ -f "$h" ] && echo "  $(basename "$h")"
done
echo
echo "skip a hook once with: git commit --no-verify"
