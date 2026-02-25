---
title: Adding Parser Adapters
---

When a source format is nested/custom, contributors should add a parser adapter.

Use this workflow:

1. Add a source entry in `registry/sources.toml` with:
- `format` (`jsonl` or `json`)
- `parser_hint` (example: `tandem_v1`)
- bounded `roots` + strict `globs`

2. Implement parser mapping in:
- `crates/trace-share-core/src/parser.rs`

3. Add tests:
- parser extraction + role mapping
- redaction fixture behavior
- dry-run export sanity (`would_upload_docs > 0` for sample input)

4. Validate:

```bash
cargo test -p trace-share-core
trace-share run --source <id> --dry-run --review --export-payload ./out/<id>.jsonl
```

Detailed in-repo guide:

- [Open `docs/PARSERS.md`](https://github.com/frumu-ai/trace-share/blob/main/docs/PARSERS.md)
