---
title: CLI Install and Usage
---

## Install

From source (Rust):

```bash
cargo install --path crates/trace-share-cli
```

From npm (prebuilt binaries):

```bash
npm i -g @frumu-ai/trace-share
```

Use local workspace binary directly (no install):

```bash
./target/release/trace-share --help
./target/release/trace-share run --dry-run --review
```

## First Run

Create local config:

```bash
mkdir -p ~/.trace-share
cp examples/config.toml ~/.trace-share/config.toml
cp examples/sources.toml ~/.trace-share/sources.toml
cp examples/policy.toml ~/.trace-share/policy.toml
```

Initialize consent/license:

```bash
trace-share consent init --license CC0-1.0
trace-share consent status
```

Source behavior:

- Built-in sources include `codex_cli`, `claude_code`, `vscode_global_storage`, and `tandem_sessions`.
- `trace-share sources add ...` persists to `~/.trace-share/sources.toml`; you do not need to re-add on each run.
- Repo `registry/sources.toml` is not auto-used for local runs unless you set `TRACE_SHARE_SOURCES_PATH` (or enable remote registry config).

## Safe Dry-Run

```bash
trace-share run --dry-run --review
```

Dry-run prints upload payload size after sanitization:

- `would_upload_docs`
- `would_upload_bytes` (raw bytes + human readable)

Print per-source byte breakdown:

```bash
trace-share run --dry-run --review --explain-size
```

Show payload preview without upload:

```bash
trace-share run --dry-run --show-payload --preview-limit 5
```

Export sanitized would-upload episodes to JSONL for inspection:

```bash
trace-share run --dry-run --review --export-payload ./out/payload.jsonl
```

Inspect exported payload locally:

```bash
wc -l ./out/payload.jsonl
jq . ./out/payload.jsonl | less
```

Example output:

```text
would_upload_docs=42
would_upload_bytes=10810412 (10.31 MiB)
uploaded_docs=0
uploaded_bytes=0 (0 B)
```

These numbers are the data that would be sent by `trace-share`, not the size of your full `~/.codex` directory.

## Upload

```bash
trace-share run --yes --review
```

Uploads require both flags:

- `--review` to inspect redaction/report context
- `--yes` to confirm network upload

By default uploads are allowlist-mode summaries (raw transcript content omitted).
Include raw content only with explicit opt-in:

```bash
trace-share run --review --yes --include-raw
```

## Full History Example (`codex_cli`)

Reprocess all history from scratch and export all would-upload episodes:

```bash
trace-share reset --all --yes
trace-share consent init --license CC0-1.0
trace-share run --source codex_cli --dry-run --review --explain-size --export-payload ./out/payload-full.jsonl
wc -l ./out/payload-full.jsonl
```

Export raw dialogue/assistant text (when permitted by sanitization gate):

```bash
trace-share run --source codex_cli --dry-run --review --include-raw --export-payload ./out/payload-full-raw.jsonl
```

Long session files are split into multiple episode records using turn/window boundaries, so exports reflect full history more granularly than one-file-one-record.

Use batch limits for heavy runs:

```bash
trace-share run --review --yes --max-upload-bytes 50000000
```

On real uploads, CLI prints:

- `uploaded_docs`
- `uploaded_bytes` (actual payload sent in this run)

## Redaction Smoke Test

Create a fixture with planted secrets and verify sanitizer output:

```bash
cat > /tmp/redaction-fixture.jsonl <<'EOF'
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:00Z","kind":"user_msg","text":"token=abc123 email=test@example.com ip=127.0.0.1 path=/home/user/private authorization: bearer MYTOKEN123"}
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:01Z","kind":"system","text":"-----BEGIN PRIVATE KEY-----\nABCDEF123456\n-----END PRIVATE KEY-----"}
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:02Z","kind":"assistant_msg","text":"jwt eyJhbGciOiJIUzI1NiJ9.cGF5bG9hZA.sigvalue1234567890"}
EOF

trace-share sanitize --in /tmp/redaction-fixture.jsonl --out /tmp/redaction-out
cat /tmp/redaction-out/redaction_report.json
cat /tmp/redaction-out/sanitized_events.jsonl
```

Run sanitizer tests:

```bash
cargo test -p trace-share-core sanitize::tests::redacts_known_patterns
cargo test -p trace-share-core sanitize::tests::redacts_jwt_pem_and_entropy
```

## Snapshot Commands

```bash
trace-share snapshot build --version 0.1.0 --in /path/to/episodes --out ./dist
trace-share snapshot publish --version 0.1.0 --from ./dist --dry-run
```
