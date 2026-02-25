# Dataset License

## Scope Separation

Code and dataset artifacts are licensed separately:

- Repository code: `MIT OR Apache-2.0`.
- Dataset artifacts (Episodes/Snapshots): `CC0-1.0` by default, unless a record explicitly declares another supported dataset license.

This separation is intentional. Training pipelines should read dataset license metadata from Snapshot manifests and per-record fields.

## Supported Dataset Licenses

Allowed dataset license values:

- `CC0-1.0` (default)
- `CC-BY-4.0` (optional, when enabled by policy)

All other dataset license values are prohibited for upload.

## Default Dataset License

Default recommendation: **CC0-1.0** for maximum compatibility with open ML training and redistribution.

Why:

- minimal legal friction for trainers
- broad interoperability across OSS model workflows

Full legal text: `licenses/CC0-1.0.txt`.

## Public Searchability and Distribution

Contributors must understand that accepted uploads may become publicly accessible:

- metadata may be publicly searchable in the index
- Episodes/Snapshots may be publicly downloadable
- contributors must consent to both searchability and training distribution

## Contributor Requirements

Before upload, contributors must:

- explicitly opt in
- confirm they have rights to share submitted data
- select/confirm the dataset license in CLI flow
- affirm they are not uploading employer/client confidential or otherwise restricted code/content

No upload should proceed without explicit consent and license capture.

## Per-Record Overrides

If non-default licenses are supported:

- each Episode must include a machine-readable license field
- Snapshot manifest must summarize license counts and exceptions
- tooling must preserve license metadata during export
- supported values are `CC0-1.0` and `CC-BY-4.0` (if enabled)

## No Warranty / Risk Notice

Sanitization lowers risk but may miss sensitive content. Dataset users should apply their own governance and filtering policies before training.

See:

- `docs/SECURITY.md` for sanitization policy and incident response
- `docs/REVOCATION.md` for removal/revocation behavior and future snapshot exclusions
