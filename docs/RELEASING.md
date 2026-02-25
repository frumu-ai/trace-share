# Releasing

This repo uses tag-driven GitHub releases and automated registry publishing (crates.io + npm).

## 1) Prepare the Version

Use the version bump script (single source for Rust + npm metadata):

```bash
./scripts/bump-version.sh 0.0.2
```

Update changelog:

- Add a new section in `CHANGELOG.md` for `v0.0.2`.

Commit:

```bash
git add Cargo.toml crates/trace-share-cli/Cargo.toml packages/trace-share/package.json CHANGELOG.md .github/workflows/publish-registries.yml .github/workflows/publish-snapshot.yml
git commit -m "Release v0.0.2"
```

## 2) Preflight Checks

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p trace-share-cli
./scripts/check-docs-commands.sh
```

Optional dry-runs:

```bash
cargo publish -p trace-share-core --dry-run
cd packages/trace-share && npm publish --dry-run --access public
```

## 3) Push Branch and Tag

```bash
git push origin main
git tag -a v0.0.2 -m "v0.0.2"
git push origin v0.0.2
```

Pushing the tag triggers `.github/workflows/release.yml`.
After `Release` completes successfully, `.github/workflows/publish-registries.yml` runs automatically and publishes crates/npm (using `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN`).

## 4) What Release Workflow Does

The `release.yml` workflow:

- creates a draft GitHub Release for the tag
- generates release notes from `CHANGELOG.md`
- builds platform binaries (Linux/macOS/Windows)
- uploads release assets (archives + npm binary assets + checksums)
- publishes the release (marks draft false)

## 5) Publish to crates.io (Fallback Manual)

Publish in dependency order:

```bash
cargo publish -p trace-share-core
# wait until crates.io index updates
cargo publish -p trace-share-cli
```

Requirements:

- `cargo login <CRATES_IO_TOKEN>` done on your machine

## 6) Publish npm Wrapper (Fallback Manual)

```bash
cd packages/trace-share
npm login
npm publish --access public
```

Package name:

- `@frumu/trace-share`

## 7) Post-Release Checks

- Verify GitHub release assets are present for all targets.
- Verify crates are visible on crates.io.
- Verify npm package is visible and installable:

```bash
npm i -g @frumu/trace-share
trace-share --help
```
