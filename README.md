# trace-share

An open-source proposal for building the public data infrastructure behind better coding-agent models.

[![CI](https://github.com/frumu-ai/trace-share/actions/workflows/ci.yml/badge.svg)](https://github.com/frumu-ai/trace-share/actions/workflows/ci.yml)
[![Registry Schema](https://github.com/frumu-ai/trace-share/actions/workflows/registry-schema.yml/badge.svg)](https://github.com/frumu-ai/trace-share/actions/workflows/registry-schema.yml)
[![Docs Site](https://github.com/frumu-ai/trace-share/actions/workflows/docs-site.yml/badge.svg)](https://github.com/frumu-ai/trace-share/actions/workflows/docs-site.yml)

## Proposal in One Line

`trace-share` is an opt-in pipeline that turns locally sanitized coding-agent traces into:
- searchable public Episodes for curation, and
- versioned Snapshot datasets for open model training.

## Why This Matters

Open-source coding models lack enough high-quality, real-world agent traces.
Most available data is static code, not iterative tool-using behavior.

That gap hurts model quality on real tasks:
- debugging
- multi-step planning
- tool invocation
- patch/test/fix loops

`trace-share` is designed to close that gap with privacy controls, reproducibility, and clear licensing.

## What We Are Building

A contributor-first data pipeline:
1. Local collection from approved sources.
2. Mandatory local sanitization before any network transfer.
3. Canonical Episode generation.
4. Artifact storage (R2/HF-compatible).
5. Public index of metadata + pointers for discovery.
6. Versioned Snapshot releases for trainers (`train/val`, checksums, manifests, data card).

Key distinction:
- Index is for search/discovery.
- Snapshot is the actual training dataset.

## System Flow

```mermaid
flowchart TD
    A[Local Source Logs<br/>codex/claude/vscode/tandem] --> B[Source Resolver<br/>built-ins + local sources.toml + remote registry]
    B --> C[Parser Layer<br/>generic + parser_hint adapters]
    C --> D[Sanitizer<br/>gitleaks if available + built-in redaction]
    D --> E{Sensitive pattern<br/>still present?}
    E -- yes --> X[Drop/Refuse Record]
    E -- no --> F[Episode Builder<br/>split into episode windows]
    F --> G[Consent + License Gate]
    G --> H[Dry-run Stats + Preview<br/>would_upload_docs/bytes]
    G --> I[Upload Gate<br/>--review + --yes]
    I --> J[Artifact Upload<br/>worker/R2]
    J --> K[Index Pointer Upsert<br/>Upstash]
```

```mermaid
flowchart LR
    A[Episodes JSONL] --> B[snapshot build]
    B --> C[train.jsonl.zst]
    B --> D[val.jsonl.zst]
    B --> E[sft.jsonl.zst]
    B --> F[tooltrace.jsonl.zst]
    B --> G[manifest + checksums + data card]
    C --> H[snapshot publish]
    D --> H
    E --> H
    F --> H
    G --> H
    H --> I[artifact store]
    H --> J[index latest pointer]
```

## Quickstart (CLI)

Build and run locally:

```bash
cargo build --release -p trace-share-cli
./target/release/trace-share --help
```

Consent and dataset license must be initialized before `run`/`publish`:

```bash
./target/release/trace-share consent init --license CC0-1.0
./target/release/trace-share consent status
./target/release/trace-share run --dry-run --review
./target/release/trace-share run --dry-run --review --explain-size
./target/release/trace-share run --dry-run --review --export-payload ./out/payload.jsonl
./target/release/trace-share run --yes --review
```

Supported dataset licenses in CLI consent flow:
- `CC0-1.0` (default)
- `CC-BY-4.0`

Source behavior:
- Built-ins include `codex_cli`, `claude_code`, `vscode_global_storage`, and `tandem_sessions`.
- `trace-share sources add ...` is persistent (`~/.trace-share/sources.toml`) and does not need to be repeated each run.
- Repo `registry/sources.toml` is not auto-used for local runs unless configured via `TRACE_SHARE_SOURCES_PATH` or remote registry settings.

Default upload mode is allowlist-first (summary-only). Raw transcript content requires explicit opt-in:

```bash
./target/release/trace-share run --review --yes --include-raw
```

Inspect exactly what would be uploaded:

```bash
./target/release/trace-share run --dry-run --review --export-payload ./out/payload.jsonl
wc -l ./out/payload.jsonl
jq . ./out/payload.jsonl | less
```

Optional terminal preview:

```bash
./target/release/trace-share run --dry-run --review --show-payload --preview-limit 3
```

Full Codex history re-scan (from scratch) with export:

```bash
./target/release/trace-share reset --all --yes
./target/release/trace-share consent init --license CC0-1.0
./target/release/trace-share run --source codex_cli --dry-run --review --explain-size --export-payload ./out/payload-full.jsonl
wc -l ./out/payload-full.jsonl
```

Include raw dialogue/assistant text in export:

```bash
./target/release/trace-share run --source codex_cli --dry-run --review --include-raw --export-payload ./out/payload-full-raw.jsonl
```

`trace-share` now splits long session files into multiple episode records (turn/window boundaries), so a single large session can produce many docs.

## Redaction Smoke Test

Create a local fixture with planted sensitive strings, then run sanitizer output/report checks:

```bash
cat > /tmp/redaction-fixture.jsonl <<'EOF'
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:00Z","kind":"user_msg","text":"token=abc123 email=test@example.com ip=127.0.0.1 path=/home/user/private authorization: bearer MYTOKEN123"}
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:01Z","kind":"system","text":"-----BEGIN PRIVATE KEY-----\nABCDEF123456\n-----END PRIVATE KEY-----"}
{"source":"manual","session_id":"s1","ts":"2026-02-25T00:00:02Z","kind":"assistant_msg","text":"jwt eyJhbGciOiJIUzI1NiJ9.cGF5bG9hZA.sigvalue1234567890"}
EOF

./target/release/trace-share sanitize --in /tmp/redaction-fixture.jsonl --out /tmp/redaction-out
cat /tmp/redaction-out/redaction_report.json
cat /tmp/redaction-out/sanitized_events.jsonl
```

Run sanitizer unit tests:

```bash
cargo test -p trace-share-core sanitize::tests::redacts_known_patterns
cargo test -p trace-share-core sanitize::tests::redacts_jwt_pem_and_entropy
```

## Why Companies Benefit

Participating organizations get:
- Better open tooling and models that everyone can build on.
- A standardized, auditable path to contribute data responsibly.
- Shared infrastructure costs across the ecosystem instead of duplicated internal effort.
- Public technical credibility through support of transparent dataset governance.

For model teams specifically:
- Structured, episode-level training data.
- Deterministic releases for reproducible experiments.
- Better signal for agentic workflows than raw chat logs.

## Why the Ecosystem Benefits

The open-source community gets:
- A common dataset format for coding-agent training.
- Public release artifacts with manifests/checksums.
- Clear consent, licensing, and revocation workflows.
- Community-governed source adapters and safety rules.

## Where Partners Can Contribute Resources

We are looking for practical support in:
- Object storage and egress credits for artifacts/snapshots.
- Search/index hosting credits.
- Optional CI/release capacity for large snapshot builds (self-hosted runners, extra minutes, faster release cadence).
- Security/privacy review support for sanitization and incident response.

Partners do not need access to unsanitized contributor data.
Baseline CI runs in this public GitHub repo; added compute support is mainly for scale and reliability as dataset volume grows.

## Trust and Safety Model

- Sanitization is mandatory and fail-closed.
- Sanitization uses layered detection (`gitleaks` when available + built-in fallback scrubber).
- Consent and license are required before upload.
- Uploads require explicit `--review` and `--yes`.
- Revocation/removal is supported.
- Data is pseudonymized where appropriate.
- Snapshot releases are versioned and auditable.

## Licensing

- Code: `MIT OR Apache-2.0`
- Dataset artifacts (Episodes/Snapshots): `CC0-1.0` by default (see `docs/DATA_LICENSE.md`)

## Current Status

Core pipeline, snapshot build/publish, registry validation CI, and npm/crate distribution flows are implemented.
Detailed roadmap and partner context: [docs/PROPOSAL.md](docs/PROPOSAL.md)

## Read Next

- Partner brief: [docs/PROPOSAL.md](docs/PROPOSAL.md)
- Dataset specification: [docs/DATASET.md](docs/DATASET.md)
- Security model: [docs/SECURITY.md](docs/SECURITY.md)
- Consent model: [docs/CONSENT.md](docs/CONSENT.md)
- Governance: [docs/GOVERNANCE.md](docs/GOVERNANCE.md)
- Parser adapters: [docs/PARSERS.md](docs/PARSERS.md)
- Releasing runbook: [docs/RELEASING.md](docs/RELEASING.md)
- Docs index: [docs/README.md](docs/README.md)
