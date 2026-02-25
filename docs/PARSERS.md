# Parser Adapter Guide

This guide explains how to add support for a new CLI/tool transcript format.

## When A Custom Parser Is Needed

Use the generic parser first. Add a custom adapter when any of these appear:

- dialogue text is missing/empty in exports
- roles are wrong (`user` vs `assistant`)
- tool calls/results are not captured
- one source file becomes a low-signal blob with poor boundaries

## Adapter Model

`trace-share` source definitions use:

- `format`: `jsonl` or `json`
- `parser_hint`: adapter selector (example: `tandem_v1`)

Implementation happens in parser code:

- add parser entry in `crates/trace-share-core/src/parser.rs`
- map source data into `CanonicalEvent`
- ensure timestamps, role/kind, and text extraction are deterministic

## Step-by-Step

1. Add/update source in `registry/sources.toml`:
- choose bounded `roots`
- set strict `globs`
- set `format`
- set `parser_hint`
- prefer `requires_opt_in = true` for new sources

2. Implement parser branch:
- parse file shape (`json` object/array or `jsonl`)
- extract text from nested message parts
- map kinds (`user_msg`, `assistant_msg`, `tool_call`, `tool_result`, `error`, `system`)
- fill `CanonicalEvent.meta` safely (cwd/model/exit_code when available)

3. Add tests:
- parser fixture test for message extraction and role mapping
- redaction behavior test with planted sensitive values
- end-to-end dry-run test to confirm non-empty `would_upload_docs`

4. Validate locally:

```bash
cargo test -p trace-share-core
./target/release/trace-share run --source <id> --dry-run --review --export-payload ./out/<id>.jsonl
```

5. Update docs:
- add source and parser behavior notes in docs/README or CLI docs
- include any caveats (unsupported fields, expected data boundaries)

## Quality Bar

New adapters should produce:

- non-empty prompt/result for meaningful sessions
- bounded episode sizes (split by turns/windows when needed)
- strong redaction coverage and fail-closed behavior

If quality is low, do not merge adapter changes until parser mapping improves.
