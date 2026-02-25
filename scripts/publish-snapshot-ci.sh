#!/usr/bin/env bash
set -euo pipefail

VERSION="${SNAPSHOT_VERSION:-}"
INPUT_PATH="${SNAPSHOT_INPUT_PATH:-}"
OUT_DIR="${SNAPSHOT_OUT_DIR:-dist/snapshots}"
SPLIT_SEED="${SNAPSHOT_SPLIT_SEED:-trace-share-split-v1}"
PUBLISH="${SNAPSHOT_PUBLISH:-false}"
DRY_RUN="${SNAPSHOT_DRY_RUN:-true}"
LOG_FILE="${PUBLISH_SNAPSHOT_LOG:-publish-snapshot.log}"
META_FILE="${SNAPSHOT_META_PATH:-snapshot-publish-meta.json}"

if [[ -z "$VERSION" ]]; then
  echo "SNAPSHOT_VERSION is required" >&2
  exit 1
fi
if [[ -z "$INPUT_PATH" ]]; then
  echo "SNAPSHOT_INPUT_PATH is required" >&2
  exit 1
fi
if [[ ! -e "$INPUT_PATH" ]]; then
  echo "SNAPSHOT_INPUT_PATH does not exist: $INPUT_PATH" >&2
  exit 1
fi

: > "$LOG_FILE"
rm -f "$META_FILE"

echo "Building snapshot version=$VERSION from=$INPUT_PATH out=$OUT_DIR" | tee -a "$LOG_FILE"
cargo run -p trace-share-cli -- snapshot build \
  --version "$VERSION" \
  --in "$INPUT_PATH" \
  --out "$OUT_DIR" \
  --split-seed "$SPLIT_SEED" 2>&1 | tee -a "$LOG_FILE"

SNAPSHOT_DIR="$OUT_DIR/dataset-$VERSION"
if [[ ! -d "$SNAPSHOT_DIR" ]]; then
  echo "Expected snapshot directory missing: $SNAPSHOT_DIR" >&2
  exit 1
fi

echo "Built snapshot dir: $SNAPSHOT_DIR" | tee -a "$LOG_FILE"

if [[ "$PUBLISH" != "true" ]]; then
  echo "Publish disabled (SNAPSHOT_PUBLISH=$PUBLISH)" | tee -a "$LOG_FILE"
  cat > "$META_FILE" <<EOF
{"version":"$VERSION","snapshot_dir":"$SNAPSHOT_DIR","published":false,"dry_run":false,"object_prefix":null}
EOF
  exit 0
fi

if [[ "$DRY_RUN" == "true" ]]; then
  echo "Publishing snapshot in dry-run mode" | tee -a "$LOG_FILE"
  cargo run -p trace-share-cli -- snapshot publish \
    --version "$VERSION" \
    --from "$OUT_DIR" \
    --dry-run 2>&1 | tee -a "$LOG_FILE"
  cat > "$META_FILE" <<EOF
{"version":"$VERSION","snapshot_dir":"$SNAPSHOT_DIR","published":false,"dry_run":true,"object_prefix":null}
EOF
  exit 0
fi

if [[ -z "${TRACE_SHARE_WORKER_BASE_URL:-}" ]]; then
  echo "TRACE_SHARE_WORKER_BASE_URL is required for live publish" >&2
  exit 1
fi
if [[ -z "${UPSTASH_VECTOR_REST_URL:-}" || -z "${UPSTASH_VECTOR_REST_TOKEN:-}" ]]; then
  echo "UPSTASH_VECTOR_REST_URL and UPSTASH_VECTOR_REST_TOKEN are required for live publish" >&2
  exit 1
fi

echo "Publishing snapshot with live worker/upstash calls" | tee -a "$LOG_FILE"
cargo run -p trace-share-cli -- snapshot publish \
  --version "$VERSION" \
  --from "$OUT_DIR" \
  --yes 2>&1 | tee -a "$LOG_FILE"

object_prefix="$(grep -E '^object_prefix=' "$LOG_FILE" | tail -n1 | cut -d'=' -f2- | tr -d '\r')"
if [[ -z "$object_prefix" || "$object_prefix" == "none" ]]; then
  object_prefix="null"
else
  object_prefix="\"$object_prefix\""
fi

cat > "$META_FILE" <<EOF
{"version":"$VERSION","snapshot_dir":"$SNAPSHOT_DIR","published":true,"dry_run":false,"object_prefix":$object_prefix}
EOF
