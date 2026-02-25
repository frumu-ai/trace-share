# Governance

## Source Registry Rules

The source registry defines what the collector may read.

Mandatory rules:

- Use bounded globs only (no broad recursive wildcards over home/root).
- Restrict collection to approved safe roots.
- Ban known sensitive directories by default.
- Require review for new or expanded source scopes.
- Keep registry changes auditable in version control.

Safe-root examples:

- specific project directories
- explicit CLI transcript/export directories

Non-examples:

- `/`, `/home`, full user profile roots
- unbounded patterns like `**/*` across mixed-trust locations

## Change Review Requirements

Registry or adapter changes require:

- motivation and expected data shape
- privacy impact note
- example test fixture (sanitized)
- reviewer sign-off

At least one reviewer must validate that collection remains within bounded scope.

## Contributing New Source Adapters

Contribution guidelines:

- Emit canonical intermediate records before Episode assembly.
- Avoid adapter-side network calls unless explicitly required.
- Include adapter-level sanitization hooks and tests.
- Document expected input paths and failure modes.
- Preserve provenance metadata needed for audit/revocation.

Required checks:

- unit tests for parser and normalization
- sample redaction behavior test
- docs update for new adapter fields

Implementation playbook:

- see [PARSERS.md](PARSERS.md) for concrete `format`/`parser_hint` adapter steps and test requirements.

## Policy Evolution

- Version sanitization and schema policies.
- Prefer additive schema changes.
- Record breaking changes in release notes and manifest.

## Community Expectations

- Privacy and consent take priority over volume.
- Conservative defaults are preferred to permissive collection.
- If unsure about safety, do not ingest until reviewed.
