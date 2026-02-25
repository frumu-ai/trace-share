use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    collections::HashSet,
    env, fs,
    path::{Component, Path, PathBuf},
};
use tracing::warn;

use crate::config::{
    AppConfig, data_dir, default_sources_path, validate_network_url, write_private_file,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    pub id: String,
    pub display_name: Option<String>,
    pub roots: Vec<String>,
    pub globs: Vec<String>,
    pub format: String,
    pub parser_hint: Option<String>,
    pub platforms: Option<Vec<String>>,
    pub requires_opt_in: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceManifest {
    pub version: Option<u32>,
    pub sources: Vec<SourceDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRegistry {
    fetched_at: DateTime<Utc>,
    etag: Option<String>,
    manifest: SourceManifest,
}

pub fn builtin_sources() -> Vec<SourceDef> {
    vec![
        SourceDef {
            id: "codex_cli".to_string(),
            display_name: Some("Codex CLI".to_string()),
            roots: vec!["~/.codex/sessions".to_string()],
            globs: vec!["**/*".to_string()],
            format: "jsonl".to_string(),
            parser_hint: Some("codex_cli_v1".to_string()),
            platforms: None,
            requires_opt_in: Some(false),
        },
        SourceDef {
            id: "claude_code".to_string(),
            display_name: Some("Claude Code".to_string()),
            roots: vec!["~/.claude/projects".to_string()],
            globs: vec!["**/sessions/*.jsonl".to_string()],
            format: "jsonl".to_string(),
            parser_hint: Some("claude_code_v1".to_string()),
            platforms: None,
            requires_opt_in: Some(false),
        },
        SourceDef {
            id: "vscode_global_storage".to_string(),
            display_name: Some("VS Code Global Storage".to_string()),
            roots: vec![
                "~/.config/Code/User/globalStorage".to_string(),
                "~/Library/Application Support/Code/User/globalStorage".to_string(),
                "~/AppData/Roaming/Code/User/globalStorage".to_string(),
            ],
            globs: vec!["**/*.jsonl".to_string(), "**/*.json".to_string()],
            format: "jsonl".to_string(),
            parser_hint: Some("vscode_storage_v1".to_string()),
            platforms: None,
            requires_opt_in: Some(false),
        },
        SourceDef {
            id: "tandem_sessions".to_string(),
            display_name: Some("Tandem Sessions".to_string()),
            roots: vec![
                "~/.local/share/tandem/data/storage".to_string(),
                "~/Library/Application Support/tandem/data/storage".to_string(),
                "~/AppData/Roaming/tandem/data/storage".to_string(),
            ],
            globs: vec!["**/sessions.json".to_string()],
            format: "json".to_string(),
            parser_hint: Some("tandem_v1".to_string()),
            platforms: Some(vec![
                "linux".to_string(),
                "macos".to_string(),
                "windows".to_string(),
            ]),
            requires_opt_in: Some(true),
        },
    ]
}

pub async fn resolve_sources(config: &AppConfig) -> Result<Vec<SourceDef>> {
    let mut merged = builtin_sources();

    if config.remote_registry.enabled {
        if let Ok(remote) = load_remote_registry(config).await {
            merged.extend(remote.sources);
        }
    }

    if let Some(local) = load_local_sources(config)? {
        merged.extend(local.sources);
    }

    let merged = merge_with_override(merged);
    let mut valid = Vec::new();
    for source in merged {
        match validate_source(&source) {
            Ok(()) => valid.push(source),
            Err(e) => warn!("skipping unsafe source {}: {e}", source.id),
        }
    }
    Ok(valid)
}

fn merge_with_override(input: Vec<SourceDef>) -> Vec<SourceDef> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for source in input.into_iter().rev() {
        if seen.insert(source.id.clone()) {
            out.push(source);
        }
    }
    out.reverse();
    out
}

pub fn load_local_sources(config: &AppConfig) -> Result<Option<SourceManifest>> {
    let p = config
        .sources_path
        .clone()
        .unwrap_or(default_sources_path()?);
    if !p.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&p)
        .with_context(|| format!("failed reading sources file {}", p.display()))?;
    let manifest = toml::from_str::<SourceManifest>(&text).context("invalid local sources.toml")?;
    validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

pub fn add_local_source(config: &AppConfig, source: SourceDef) -> Result<PathBuf> {
    validate_source(&source)?;
    let path = config
        .sources_path
        .clone()
        .unwrap_or(default_sources_path()?);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut manifest = if path.exists() {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed reading sources file {}", path.display()))?;
        toml::from_str::<SourceManifest>(&text).context("invalid local sources.toml")?
    } else {
        SourceManifest {
            version: Some(1),
            sources: Vec::new(),
        }
    };

    if let Some(existing) = manifest.sources.iter_mut().find(|s| s.id == source.id) {
        *existing = source;
    } else {
        manifest.sources.push(source);
    }

    manifest.sources.sort_by(|a, b| a.id.cmp(&b.id));
    let text = toml::to_string_pretty(&manifest)?;
    write_private_file(&path, text.as_bytes())?;
    Ok(path)
}

pub async fn load_remote_registry(config: &AppConfig) -> Result<SourceManifest> {
    let url = config
        .remote_registry
        .url
        .clone()
        .context("remote registry url missing")?;
    validate_network_url(&url, "remote registry")?;

    let cache_path = data_dir()?.join("registry-cache.json");
    let cached = read_cache(&cache_path).ok();
    let ttl_hours = config.remote_registry.cache_ttl_hours.max(1);

    if let Some(cached) = &cached {
        let age = Utc::now() - cached.fetched_at;
        if age.num_hours() < ttl_hours as i64 {
            return Ok(cached.manifest.clone());
        }
    }

    let client = reqwest::Client::new();
    let mut req = client.get(&url);
    if let Some(cached) = &cached {
        if let Some(etag) = &cached.etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, etag);
        }
    }

    let resp = req.send().await?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        if let Some(cached) = cached {
            return Ok(cached.manifest);
        }
    }

    let status = resp.status();
    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if !status.is_success() {
        if let Some(cached) = cached {
            return Ok(cached.manifest);
        }
        anyhow::bail!("remote registry fetch failed: {status}");
    }

    let body = resp.text().await?;
    let manifest = toml::from_str::<SourceManifest>(&body).context("invalid remote manifest")?;
    validate_manifest(&manifest)?;
    let snapshot = CachedRegistry {
        fetched_at: Utc::now(),
        etag,
        manifest: manifest.clone(),
    };
    let snapshot_bytes = serde_json::to_vec_pretty(&snapshot)?;
    write_private_file(&cache_path, &snapshot_bytes)?;
    Ok(manifest)
}

fn read_cache(path: &Path) -> Result<CachedRegistry> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn discover_files(source: &SourceDef) -> Result<Vec<PathBuf>> {
    validate_source(source)?;
    let mut files = Vec::new();
    let max_files = 5000usize;
    let max_file_bytes = 20 * 1024 * 1024u64;

    for root in &source.roots {
        let expanded = expand_tilde(root);
        if !is_root_allowlisted(&expanded) {
            continue;
        }
        if !expanded.exists() {
            continue;
        }

        for entry in ignore::WalkBuilder::new(&expanded)
            .hidden(false)
            .git_ignore(false)
            .build()
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().map(|f| f.is_file()).unwrap_or(false) {
                continue;
            }

            let relative = entry.path().strip_prefix(&expanded).unwrap_or(entry.path());
            if matches_any_glob(relative, &source.globs) {
                if let Ok(md) = entry.metadata() {
                    if md.len() > max_file_bytes {
                        continue;
                    }
                }
                files.push(entry.path().to_path_buf());
                if files.len() >= max_files {
                    break;
                }
            }
        }
        if files.len() >= max_files {
            break;
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn matches_any_glob(path: &Path, globs: &[String]) -> bool {
    let path_text = path.to_string_lossy();
    globs
        .iter()
        .filter_map(|g| Pattern::new(g).ok())
        .any(|p| p.matches(&path_text))
}

pub fn validate_manifest(manifest: &SourceManifest) -> Result<()> {
    if manifest.sources.is_empty() {
        anyhow::bail!("manifest must contain at least one source");
    }
    if let Some(version) = manifest.version {
        if version != 1 {
            anyhow::bail!("unsupported manifest version: {version} (expected 1)");
        }
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for source in &manifest.sources {
        *counts.entry(source.id.as_str()).or_insert(0) += 1;
        validate_source(source)?;
    }

    let duplicates = counts
        .into_iter()
        .filter_map(|(id, n)| if n > 1 { Some(id.to_string()) } else { None })
        .collect::<Vec<_>>();
    if !duplicates.is_empty() {
        anyhow::bail!(
            "duplicate source ids in manifest: {}",
            duplicates.join(", ")
        );
    }

    Ok(())
}

pub fn validate_source(source: &SourceDef) -> Result<()> {
    if source.id.trim().is_empty() {
        anyhow::bail!("source id cannot be empty");
    }
    let valid_id = source
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    if !valid_id {
        anyhow::bail!("source id contains invalid characters");
    }

    if source.roots.is_empty() {
        anyhow::bail!("source must declare at least one root");
    }
    if source.globs.is_empty() {
        anyhow::bail!("source must declare at least one glob");
    }
    if source.format.trim().is_empty() {
        anyhow::bail!("source format cannot be empty");
    }
    if !matches!(source.format.as_str(), "jsonl" | "json" | "mixed") {
        anyhow::bail!("source format must be one of: jsonl, json, mixed");
    }

    for root in &source.roots {
        let expanded = expand_tilde(root);
        if has_parent_traversal(&expanded) {
            anyhow::bail!("root has path traversal segments");
        }
        if !is_root_allowlisted(&expanded) {
            anyhow::bail!(
                "root is outside allowlisted user locations: {}",
                expanded.display()
            );
        }
    }

    for g in &source.globs {
        Pattern::new(g).with_context(|| format!("invalid glob pattern: {g}"))?;
        if g.contains("..") {
            anyhow::bail!("glob cannot include parent traversal");
        }
    }

    Ok(())
}

fn has_parent_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn is_root_allowlisted(path: &Path) -> bool {
    allowlisted_roots()
        .into_iter()
        .any(|root| path_starts_with(path, &root))
}

fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" || input.starts_with("~/") || input.starts_with("~\\") {
        if let Some(home) = resolve_home_dir() {
            return PathBuf::from(input.replacen('~', home.to_string_lossy().as_ref(), 1));
        }
    }
    PathBuf::from(input)
}

fn allowlisted_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = resolve_home_dir() {
        roots.push(home);
    }
    if let Ok(appdata) = env::var("APPDATA") {
        roots.push(PathBuf::from(appdata));
    }
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(local_appdata));
    }
    roots
}

fn resolve_home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| env::var("USERPROFILE").ok().map(PathBuf::from))
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let p = path.to_string_lossy().to_lowercase();
        let r = root.to_string_lossy().to_lowercase();
        return p == r
            || p.strip_prefix(&(r.clone() + "\\")).is_some()
            || p.strip_prefix(&(r + "/")).is_some();
    }

    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::AppConfig;

    use super::{
        SourceDef, SourceManifest, add_local_source, load_local_sources, validate_manifest,
        validate_source,
    };

    #[test]
    fn add_local_source_persists_manifest() {
        let mut cfg = AppConfig::default();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        let test_path = std::env::temp_dir().join(format!("trace-share-sources-{nonce}.toml"));
        cfg.sources_path = Some(test_path.clone());

        let src = SourceDef {
            id: "demo_source".to_string(),
            display_name: Some("Demo".to_string()),
            roots: vec!["~/demo".to_string()],
            globs: vec!["**/*.jsonl".to_string()],
            format: "jsonl".to_string(),
            parser_hint: Some("generic".to_string()),
            platforms: None,
            requires_opt_in: Some(true),
        };

        add_local_source(&cfg, src).expect("add source");
        let loaded = load_local_sources(&cfg)
            .expect("load manifest")
            .expect("manifest exists");
        assert!(loaded.sources.iter().any(|s| s.id == "demo_source"));

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn rejects_invalid_source_id() {
        let src = SourceDef {
            id: "bad id".to_string(),
            display_name: None,
            roots: vec!["~/demo".to_string()],
            globs: vec!["**/*.jsonl".to_string()],
            format: "jsonl".to_string(),
            parser_hint: None,
            platforms: None,
            requires_opt_in: None,
        };
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn rejects_duplicate_source_ids_in_manifest() {
        let source = SourceDef {
            id: "dup_source".to_string(),
            display_name: None,
            roots: vec!["~/.codex/sessions".to_string()],
            globs: vec!["**/*.jsonl".to_string()],
            format: "jsonl".to_string(),
            parser_hint: None,
            platforms: None,
            requires_opt_in: Some(false),
        };
        let manifest = SourceManifest {
            version: Some(1),
            sources: vec![source.clone(), source],
        };
        assert!(validate_manifest(&manifest).is_err());
    }
}
