use anyhow::{Context, Result, bail};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn ensure_secure_url(url: &str, label: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid {label} URL: {url}"))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if allow_insecure_http() => Ok(()),
        scheme => bail!(
            "{label} must use https (got {scheme}). Set TRACE_SHARE_ALLOW_INSECURE_HTTP=1 only for local testing."
        ),
    }
}

fn allow_insecure_http() -> bool {
    env::var("TRACE_SHARE_ALLOW_INSECURE_HTTP")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

pub fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = temp_path(path, nonce);

    let mut file = new_private_file(&tmp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn temp_path(path: &Path, nonce: u128) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(format!(".tmp-{nonce}"));
    PathBuf::from(p)
}

fn new_private_file(path: &Path) -> Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.create(true).truncate(true).write(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    Ok(opts.open(path)?)
}

#[cfg(test)]
mod tests {
    use super::ensure_secure_url;

    #[test]
    fn enforces_https_by_default() {
        assert!(ensure_secure_url("https://example.com", "test").is_ok());
        assert!(ensure_secure_url("http://example.com", "test").is_err());
    }
}
