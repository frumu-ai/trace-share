# trace-share Proposal

## Summary

trace-share builds an opt-in data pipeline for **sanitized coding-agent traces**. The goal is to produce high-quality open Episodes and reproducible Snapshot releases that improve open-source coding models.

This project has two distinct outputs:

1. Public search/discovery via an Index (Upstash Vector or similar).
2. Open training data via downloadable Snapshot artifacts.

The Index helps curation and discoverability. The Snapshot is the training dataset.

## Problem

Open-source coding models are constrained by limited access to real agentic traces:

- Most available code data is static source text, not multi-step tool-using behavior.
- Existing traces are often private, inconsistent, or legally unclear.
- Privacy and secret leakage concerns prevent many teams from sharing logs.
- Trainers need reproducible, versioned datasets, not ad-hoc log dumps.

Without realistic Episodes (prompt, tool actions, edits, outcomes), OSS models underperform on planning, debugging, and iterative development tasks.

## Solution

trace-share provides an end-to-end, contributor-first pipeline:

- Contributors run a local Rust CLI.
- Raw logs/files are sanitized locally (before upload).
- Data is transformed into canonical training units called **Episodes**.
- Sanitized artifacts are stored in an artifacts store (R2 or Hugging Face dataset repo).
- Searchable metadata and pointers are indexed in an **Index**.
- Periodic **Snapshot** releases package train/val splits plus manifests/checksums.

Design principles:

- Opt-in only, explicit consent.
- Conservative privacy defaults.
- Reproducible dataset releases.
- Clear licensing separation between code and dataset.

## Who Benefits

### Trainers and model teams

- Access to structured, versioned Episode snapshots.
- Easier filtering by task/tool/language/outcome.
- Better data quality than raw conversational logs.

### OSS community

- Shared public dataset for benchmarking and fine-tuning.
- Transparency through data cards and release manifests.
- Community governance for source adapters and collection rules.

### Sponsors/partners

- Clear avenue to support open model quality.
- Measurable outputs: dataset versions, documentation, demos.
- Low integration friction via standard object storage + CI workflows.

## Architecture Overview

Data plane:

1. Contributor CLI collects from approved local sources.
2. Sanitization pipeline applies regex redaction + optional secret scanners.
3. Episode builder emits canonical records and redaction report.
4. Artifacts writer uploads sanitized files to artifacts store.
5. Index writer publishes pointer metadata for search.
6. Release job composes Snapshot bundles and signatures/checksums.

Control plane:

- Source registry with bounded globs and safe roots.
- Policy configuration (allowlist, denylist, scanners).
- Consent and license capture per contributor.
- Revocation pipeline for removals from index/snapshots.

Terminology used consistently:

- **Episode**: training unit.
- **Snapshot**: versioned dataset release.
- **Index**: vector-backed searchable metadata/pointers.
- **Artifacts store**: downloadable files in R2/HF datasets.

## What We Ask From Partners

Optional support, depending on partner capacity:

- Object storage credits for Snapshot and artifact hosting.
- Vector index/search hosting credits.
- Optional build/release compute for high-volume sanitization verification and snapshot export jobs.
- Security review support for sanitization and incident handling.

No partner access to unsanitized contributor data is required.
Baseline automation runs on GitHub Actions in this public repo; partner compute is for higher throughput, tighter SLAs, or self-hosted hardening.

## What We Deliver

- Rust CLI for source collection, local sanitization, consented upload.
- Canonical Episode schema and export pipeline.
- Versioned Snapshot releases (`jsonl.zst`; Parquet is planned as a follow-on export target).
- Data card + manifest + checksums per release.
- Search/discovery demo backed by the Index.
- Governance and security documentation for transparent operations.

## Trust, Privacy, and Reproducibility

- Sanitization runs locally before any network transfer.
- Contributors must explicitly opt in and select data license terms.
- Revocation/removal requests are supported and auditable.
- We do not promise perfect sanitization; review mode and redaction reports are part of normal workflow.
- Trainers consume Snapshot files, not vectors.
