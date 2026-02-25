#!/usr/bin/env bash
set -euo pipefail

DOCS_ROOT="docs-site/src/content/docs"

required=(
  "npm i -g @frumu-ai/trace-share"
  "trace-share consent init --license CC0-1.0"
  "trace-share run --dry-run --review"
  "trace-share run --yes"
  "trace-share snapshot build --version"
  "trace-share snapshot publish --version"
)

missing=0
for cmd in "${required[@]}"; do
  if ! rg -F --quiet "$cmd" "$DOCS_ROOT"; then
    echo "Missing required docs command snippet: $cmd" >&2
    missing=1
  fi
done

if [[ $missing -ne 0 ]]; then
  echo "docs command coverage check failed" >&2
  exit 1
fi

echo "docs command coverage check passed"
