# Changelog

All notable changes to this project will be documented in this file.

## [v0.0.4] - 2026-02-25

### Security

- Enforced HTTPS-by-default for authenticated outbound requests:
  - worker upload/revocation endpoints
  - Upstash publish/index endpoints
  - remote registry fetch endpoint
- Added explicit insecure transport override for local testing only:
  - `TRACE_SHARE_ALLOW_INSECURE_HTTP=1`
- Added secure local file-write path for sensitive artifacts:
  - anonymization salt
  - registry cache
  - local `sources.toml` manifest
- Hardened sanitization coverage for Windows-style paths (`C:\...`) to prevent local path leakage in exported episodes.

### Testing

- Updated HTTP-based integration tests to use explicit insecure transport opt-in under test environment.
- Added/updated security-related test coverage and kept core test/clippy checks clean.

## [v0.0.3] - 2026-02-25

### Added

- CLI version visibility improvements:
  - `trace-share --help` now shows current version
  - `--version` exposed consistently
- Startup update-check notice that alerts users when a newer release is available and prints npm/cargo update commands.

### Release / CI

- Fixed npm publish authentication wiring in GitHub Actions.
- Added npm package repository metadata required for npm provenance verification.
- Added docs and workflow improvements for release/publish reliability.

## [v0.0.2] - 2026-02-25

### Fixed

- Windows source safety/allowlist behavior:
  - support for `USERPROFILE`/`LOCALAPPDATA` in root allowlisting
  - `~\...` and `~/...` expansion without requiring manual `HOME` setup
- Resolved Windows-first-run ingestion issues where valid local source roots were incorrectly rejected as unsafe.

### Docs / UX

- Improved docs-site routing/base-path behavior for GitHub Pages project hosting.
- Added docs landing/index page and improved top-level docs navigation.

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
