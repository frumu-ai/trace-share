# Security

## Threat Model

Primary risks:

- Secret leakage (API keys, tokens, credentials, private keys).
- Personal data leakage (emails, phone numbers, identifiers).
- Sensitive internal references accidentally included in traces.

Assumption: contributor environment may contain mixed-safe and unsafe data. Controls must fail closed where possible.

## Sanitization Policy Summary

Sanitization runs locally before network transfer.

Baseline controls:

- Regex-based redaction for common secret and PII patterns.
- Allowlist source collection (bounded roots and globs).
- Denylist for sensitive paths/extensions.
- Deterministic placeholder replacement for detected secrets.

Optional controls:

- `gitleaks` pass over candidate payloads before upload.
- Organization-specific custom regex rules.

Important limitation:

- Sanitization reduces risk but is not perfect.

## Redaction Report and Review Mode

Every run should emit a redaction report including:

- total matches by detector/rule
- affected files/trace segments
- unresolved warnings (if any)

Review mode should let contributors inspect sanitized output and decide to proceed or abort.

Recommended fail-safe behavior:

- Block upload if high-confidence secret patterns remain after sanitization.

## Revocation and Removal Process

Contributors can request revocation/removal of previously shared Episodes.

Process:

1. Submit revocation request with Episode IDs or source/run metadata.
2. Verify request authenticity and scope.
3. Remove indexed pointers/metadata from public Index.
4. Exclude revoked Episodes from the next Snapshot release.
5. Publish revocation notes in release manifest/change log.

Operational note:

- Past third-party downloads cannot be force-deleted, but future official Snapshot releases must reflect removals.

## Incident Response (Minimum)

- Triage report within defined response window.
- Temporarily pause affected ingestion paths if active leakage is suspected.
- Re-sanitize/rebuild impacted Snapshot versions.
- Publish transparent post-incident summary.
