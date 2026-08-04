#!/usr/bin/env bash
# The whole CI suite, runnable locally. .github/workflows/ci.yml calls THIS
# script rather than duplicating the commands, so local and CI cannot drift.
#
#   scripts/ci.sh              # everything
#   scripts/ci.sh rust         # cargo tests only
#   scripts/ci.sh docs         # doc checks only (fast — no Rust build)
set -euo pipefail

cd "$(dirname "$0")/.."
WHICH="${1:-all}"
FAILED=()

step() { printf '\n\033[1m=== %s\033[0m\n' "$1"; }

run_rust() {
  step "cargo test -p sim -p pyrite (incl. golden replays)"
  # The game crate is excluded: it needs Bevy's system deps and, by rule,
  # contains no sim logic.
  cargo test -p sim -p pyrite || FAILED+=("rust")
}

run_docs() {
  step "mermaid diagrams parse"
  if ! command -v node >/dev/null 2>&1; then
    echo "node not found — install Node 20+ to run the doc checks" >&2
    FAILED+=("docs (node missing)")
    return
  fi
  # Install on first run, or whenever the lockfile is newer than the tree.
  if [ ! -d scripts/node_modules ] \
     || [ scripts/package-lock.json -nt scripts/node_modules ]; then
    echo "installing doc-check deps…"
    if [ -f scripts/package-lock.json ]; then
      npm ci --prefix scripts --silent --no-fund --no-audit
    else
      npm install --prefix scripts --silent --no-fund --no-audit
    fi
  fi
  node scripts/check-mermaid.mjs docs || FAILED+=("mermaid")
}

case "$WHICH" in
  all)  run_docs; run_rust ;;   # docs first: seconds, vs. minutes for Rust
  rust) run_rust ;;
  docs) run_docs ;;
  *)    echo "usage: scripts/ci.sh [all|rust|docs]" >&2; exit 2 ;;
esac

if [ ${#FAILED[@]} -gt 0 ]; then
  printf '\n\033[31mFAILED: %s\033[0m\n' "${FAILED[*]}"
  exit 1
fi

printf '\n\033[32mAll checks passed.\033[0m\n'
