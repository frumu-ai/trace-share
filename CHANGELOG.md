# Changelog

All notable changes to this project will be documented in this file.

## [v0.0.1] - 2026-02-25

Initial public release of `trace-share`.

### Added

- Rust workspace with `trace-share-core` and `trace-share-cli`.
- End-to-end local pipeline for:
  - source discovery/resolution
  - parsing to canonical events
  - mandatory sanitization/redaction
  - episode generation (including multi-episode splitting for long sessions)
  - dedupe/state tracking and dry-run/upload flows
- Built-in source support for:
  - `codex_cli`
  - `claude_code`
  - `vscode_global_storage`
  - `tandem_sessions`
- Config and state infrastructure:
  - file + env config loading
  - SQLite state store for runs/files/uploads/consent/revocation
  - source registry resolution (built-in + local + remote)
- Consent/license and governance gates:
  - `consent init/status`
  - required acceptance before upload
  - supported dataset licenses: `CC0-1.0`, `CC-BY-4.0`
- Revocation lifecycle:
  - local revocation queue
  - sync command path
- Snapshot workflow:
  - deterministic snapshot build
  - checksums/manifests
  - derived export formats (SFT and ToolTrace)
- CLI safety and transparency features:
  - `--dry-run`, `--review`, `--yes`
  - payload preview and payload export
  - upload size counters and `--explain-size` breakdown

### Security

- Mandatory sanitization gate before publish (fail-closed).
- Layered sensitive-data detection:
  - `gitleaks` pass when available
  - built-in scrubber for secrets/tokens/JWT/PEM/high-entropy strings + email/IP/path patterns
- Source safety guardrails:
  - root allowlisting
  - traversal rejection
  - scan caps

### Tooling and Release

- Release workflow to build cross-platform binaries and npm wrapper assets.
- Automated release-note generation from `CHANGELOG.md` in release workflow.
- Docs command coverage check script for required CLI snippets.
- Registry schema CI workflow and public docs site scaffolding.

