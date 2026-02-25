# Dataset Specification

## Scope

This document defines the canonical **Episode** schema, export formats, and **Snapshot** layout for training data releases.

Key distinction:

- **Index** supports search and curation.
- **Snapshot** is the downloadable training dataset.
- Trainers consume Snapshots, not vectors.

## Canonical Episode Record

Each Episode is a single training unit.

```json
{
  "episode_id": "ep_01J...",
  "snapshot_version": "dataset-v1.2",
  "created_at": "2026-02-25T12:34:56Z",
  "contributor": {
    "contributor_id": "anon_c_...",
    "consent_version": "2026-02-25",
    "license": "CC0-1.0"
  },
  "provenance": {
    "source_adapter": "codex_cli_v1",
    "source_ref": "sha256:...",
    "repo_fingerprint": "sha256:...",
    "sanitization_policy_version": "policy-v3"
  },
  "task": {
    "prompt": "...sanitized...",
    "language": "rust",
    "tags": ["bugfix", "tests"],
    "difficulty": "medium"
  },
  "trace": {
    "messages": [],
    "tool_calls": [],
    "file_ops": [],
    "terminal_ops": []
  },
  "outcome": {
    "status": "success",
    "tests_passed": true,
    "error_summary": null
  },
  "quality": {
    "redaction_count": 2,
    "reviewed": true,
    "flags": []
  },
  "artifacts": {
    "patch": "...",
    "final_files": [],
    "logs_ref": "r2://bucket/path/or/hf://dataset/path"
  },
  "index_metadata": {
    "embedding_model": "text-embedding-3-large",
    "pointer_uri": "https://.../episode/ep_01J..."
  }
}
```

Required fields:

- `episode_id`, `created_at`, `contributor.license`, `trace`, `outcome.status`.

Recommended fields:

- `provenance.*`, `quality.*`, `index_metadata.pointer_uri`.

## Export Formats

### SFT export

Purpose: supervised fine-tuning.

- Input: sanitized instruction/context and tool history summary.
- Target: next assistant action/response and optionally structured edit output.

Suggested record keys:

- `id`, `messages`, `target`, `metadata`.

### TOOLTRACE export

Purpose: train tool-using behavior.

- Preserves stepwise actions and observations.
- Encodes tool call arguments/results and execution boundaries.

Suggested record keys:

- `id`, `steps[]`, `final_outcome`, `metadata`.

### DPO export (optional later)

Not required for initial releases.

- Add when preference pairs are available with clear provenance.

## Snapshot Layout

Release artifacts should use a versioned layout:

```text
dataset-vX.Y/
  train.jsonl.zst
  val.jsonl.zst
  train.parquet            # optional if produced
  val.parquet              # optional if produced
  manifest.json
  CHECKSUMS.txt
  DATA_CARD.md
```

`manifest.json` should include:

- snapshot version, creation timestamp
- schema version
- record counts and split sizes
- sanitizer/policy versions
- per-file checksums
- license summary (default + per-record overrides)

## Public Searchable Definition

The public **Index** stores only searchable metadata and pointers, such as:

- `episode_id`
- embedding vector
- task tags/language/tool types
- outcome label (success/failure)
- anonymized contributor/source identifiers
- pointer URI to artifact location

The Index does not replace Snapshot release files.
