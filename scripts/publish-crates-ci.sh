#!/usr/bin/env bash
set -euo pipefail

DRY_RUN=false
LOG_FILE="${PUBLISH_CRATES_LOG:-publish-crates.log}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

CRATES=(
  "crates/trace-share-core"
  "crates/trace-share-cli"
)

: > "$LOG_FILE"
for crate in "${CRATES[@]}"; do
  echo "Processing $crate" | tee -a "$LOG_FILE"

  if [[ "$DRY_RUN" == "true" ]]; then
    (cd "$crate" && cargo check) 2>&1 | tee -a "$LOG_FILE"
    continue
  fi

  set +e
  output="$(cd "$crate" && cargo publish 2>&1)"
  code=$?
  set -e

  echo "$output" | tee -a "$LOG_FILE"

  if [[ $code -ne 0 ]]; then
    if echo "$output" | grep -q "already exists on crates.io index"; then
      echo "SKIP already published: $crate" | tee -a "$LOG_FILE"
      continue
    fi
    exit $code
  fi

  sleep 8
done
