use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstashConfig {
    pub rest_url: Option<String>,
    pub rest_token: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub base_url: Option<String>,
    pub api_token: Option<String>,
    pub timeout_seconds: u64,
    pub upload_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub path: Option<PathBuf>,
    pub allowlist_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRegistryConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub cache_ttl_hours: u64,
    pub require_consent: bool,
    pub last_accepted_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub upstash: UpstashConfig,
    pub worker: WorkerConfig,
    pub policy: PolicyConfig,
    pub sources_path: Option<PathBuf>,
    pub remote_registry: RemoteRegistryConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            upstash: UpstashConfig {
                rest_url: None,
                rest_token: None,
                namespace: None,
            },
            worker: WorkerConfig {
                base_url: None,
                api_token: None,
                timeout_seconds: 30,
                upload_mode: "legacy".to_string(),
            },
            policy: PolicyConfig {
                path: None,
                allowlist_mode: true,
            },
            sources_path: None,
            remote_registry: RemoteRegistryConfig {
                enabled: false,
                url: Some("https://raw.githubusercontent.com/frumu-ai/trace-share-registry/main/registry/sources.toml".to_string()),
                cache_ttl_hours: 24,
                require_consent: true,
                last_accepted_digest: None,
            },
        }
    }
}

pub fn config_dir() -> Result<PathBuf> {
    app_home_dir()
}

pub fn data_dir() -> Result<PathBuf> {
    app_home_dir()
}

pub fn default_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn default_sources_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("sources.toml"))
}

pub fn app_home_dir() -> Result<PathBuf> {
    if let Ok(custom) = env::var("TRACE_SHARE_HOME") {
        return Ok(PathBuf::from(custom));
    }

    if cfg!(windows) {
        let dirs = ProjectDirs::from("ai", "trace-share", "trace-share")
            .context("failed to resolve Windows app directory")?;
        return Ok(dirs.data_local_dir().to_path_buf());
    }

    let home = env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".trace-share"))
}

pub fn load_config() -> Result<AppConfig> {
    let config_path = env::var("TRACE_SHARE_CONFIG")
        .ok()
        .map(PathBuf::from)
        .unwrap_or(default_config_path()?);

    let mut config = if config_path.exists() {
        let text = fs::read_to_string(&config_path)
            .with_context(|| format!("failed reading config file {}", config_path.display()))?;
        toml::from_str::<AppConfig>(&text).context("invalid config.toml format")?
    } else {
        AppConfig::default()
    };

    if let Ok(v) = env::var("UPSTASH_VECTOR_REST_URL") {
        config.upstash.rest_url = Some(v);
    }
    if let Ok(v) = env::var("UPSTASH_VECTOR_REST_TOKEN") {
        config.upstash.rest_token = Some(v);
    }
    if let Ok(v) = env::var("TRACE_SHARE_WORKER_BASE_URL") {
        config.worker.base_url = Some(v);
    }
    if let Ok(v) = env::var("TRACE_SHARE_WORKER_API_TOKEN") {
        config.worker.api_token = Some(v);
    }
    if let Ok(v) = env::var("TRACE_SHARE_WORKER_UPLOAD_MODE") {
        config.worker.upload_mode = v;
    }
    if let Ok(v) = env::var("TRACE_SHARE_NAMESPACE") {
        config.upstash.namespace = Some(v);
    }
    if let Ok(v) = env::var("TRACE_SHARE_POLICY_PATH") {
        config.policy.path = Some(PathBuf::from(v));
    }
    if let Ok(v) = env::var("TRACE_SHARE_SOURCES_PATH") {
        config.sources_path = Some(PathBuf::from(v));
    }
    if let Ok(v) = env::var("TRACE_SHARE_ALLOWLIST_MODE") {
        config.policy.allowlist_mode = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes");
    }
    if let Ok(v) = env::var("TRACE_SHARE_REMOTE_REGISTRY_ENABLED") {
        config.remote_registry.enabled = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes");
    }
    if let Ok(v) = env::var("TRACE_SHARE_REMOTE_REGISTRY_URL") {
        config.remote_registry.url = Some(v);
    }

    Ok(config)
}

pub fn ensure_dirs() -> Result<()> {
    let config_dir = config_dir()?;
    if config_dir.exists() && !config_dir.is_dir() {
        anyhow::bail!(
            "trace-share home path exists but is not a directory: {}",
            config_dir.display()
        );
    }
    fs::create_dir_all(&config_dir)?;

    let data_dir = data_dir()?;
    if data_dir.exists() && !data_dir.is_dir() {
        anyhow::bail!(
            "trace-share data path exists but is not a directory: {}",
            data_dir.display()
        );
    }
    fs::create_dir_all(&data_dir)?;
    Ok(())
}

pub fn validate_network_url(url: &str, label: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid {label} URL: {url}"))?;

    if parsed.scheme() == "https" {
        return Ok(());
    }

    if parsed.scheme() == "http" && (allow_insecure_http() || is_loopback_http(&parsed)) {
        return Ok(());
    }

    anyhow::bail!(
        "insecure {label} URL scheme '{}' is not allowed (use https; http is only allowed for loopback or when TRACE_SHARE_ALLOW_INSECURE_HTTP=1)",
        parsed.scheme()
    );
}

fn allow_insecure_http() -> bool {
    env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

fn is_loopback_http(url: &reqwest::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
