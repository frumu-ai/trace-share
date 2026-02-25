#!/usr/bin/env bash
set -euo pipefail

DRY_RUN=false
PROVENANCE=false
LOG_FILE="${PUBLISH_NPM_LOG:-publish-npm.log}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --provenance)
      PROVENANCE=true
      shift
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

PKG_DIR="packages/trace-share"
: > "$LOG_FILE"

name=$(node -p "require('./$PKG_DIR/package.json').name")
version=$(node -p "require('./$PKG_DIR/package.json').version")

echo "Publishing $name@$version" | tee -a "$LOG_FILE"
if npm view "${name}@${version}" version >/dev/null 2>&1; then
  echo "SKIP already published" | tee -a "$LOG_FILE"
  exit 0
fi

if [[ "$DRY_RUN" == "true" ]]; then
  (cd "$PKG_DIR" && npm publish --access public --dry-run) 2>&1 | tee -a "$LOG_FILE"
else
  args=(--access public)
  if [[ "$PROVENANCE" == "true" ]]; then
    args+=(--provenance)
  fi
  (cd "$PKG_DIR" && npm publish "${args[@]}") 2>&1 | tee -a "$LOG_FILE"
fi
