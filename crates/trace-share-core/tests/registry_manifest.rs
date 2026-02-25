use std::path::PathBuf;

use trace_share_core::sources::{SourceManifest, validate_manifest};

#[test]
fn validates_repo_registry_manifest() {
    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry/sources.toml");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("failed reading {}: {e}", manifest_path.display()));
    let manifest: SourceManifest = toml::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed parsing {}: {e}", manifest_path.display()));
    validate_manifest(&manifest).expect("registry/sources.toml must validate");
}
