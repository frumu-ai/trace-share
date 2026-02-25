# Revocation and Removal Policy

## Scope

This project supports revocation/removal requests for previously shared Episodes.

## What revocation means

- Index pointers/metadata are removed from public search where feasible.
- Revoked Episode IDs are excluded from future official Snapshot releases.
- Existing third-party downloads cannot be force-deleted retroactively.

## Request process

1. Submit a revocation request with Episode IDs (or source/run metadata).
2. Project maintainers verify requester authenticity and scope.
3. Revoked IDs are recorded and synced to revocation state.
4. Future Snapshot builds exclude revoked IDs.
5. Release notes/manifests document removals where appropriate.

## SLA and guarantees

- Revocation is best-effort and forward-looking.
- Future official outputs should reflect revocations.
- Historical external copies may persist beyond project control.

See also: `docs/SECURITY.md` for sanitization and incident-response context.
