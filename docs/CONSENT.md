# Consent and Contributor Acknowledgement

## Consent Goals

Consent is explicit, opt-in, and understandable without legal jargon.

Contributors must acknowledge all three points before upload:

- data is sanitized, but sanitization can miss things
- data will be publicly searchable
- data may be used for ML training and published in Snapshot releases

## Exact CLI Consent Text

Use this text (or stricter) in the CLI confirmation step:

> You are about to share sanitized coding-agent trace data.
> 
> Important:
> 1) Sanitization reduces risk, but it may miss sensitive data.
> 2) Shared data may be publicly searchable through a search index.
> 3) Shared data may be included in public dataset snapshots used for machine-learning training.
> 
> Only continue if you have the rights to share this data.
> 
> Type "I CONSENT" to continue.

## Required Interaction

- Show summary of files/sources included.
- Show redaction report summary before consent.
- Require exact acknowledgement input (`I CONSENT`) or explicit equivalent.
- Store consent timestamp + consent text version.
- Abort upload on non-acknowledgement.

## License Selection Step

Contributors choose a data license before upload:

- Recommended default: `CC0-1.0`.
- Alternative licenses may be supported, but must be recorded per Episode.
- CLI should clearly explain that more restrictive licenses reduce downstream training compatibility.

Example prompt:

> Choose dataset license for this upload:
> [1] CC0-1.0 (recommended, maximum reuse)
> [2] Another supported license

## Rights and Responsibility Reminder

Contributors must confirm:

- they have rights to share the submitted material
- no known contractual or policy restrictions are violated
- they reviewed redactions before consenting
