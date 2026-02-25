use anyhow::{Context, Result, bail};
use chrono::Utc;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use tokio::time::{Duration, sleep};

use crate::{
    config::AppConfig,
    episode::{EpisodeRecord, derive_sft, derive_tooltrace},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotBuildResult {
    pub version: String,
    pub train_count: usize,
    pub val_count: usize,
    pub out_dir: PathBuf,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPublishResult {
    pub version: String,
    pub snapshot_dir: PathBuf,
    pub object_prefix: Option<String>,
    pub indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSnapshotPublishResponse {
    pub version: String,
    pub object_prefix: Option<String>,
    pub public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotFileEntry {
    name: String,
    bytes: u64,
    sha256: String,
}

pub fn build_snapshot(
    version: &str,
    input: &Path,
    out_root: &Path,
    split_seed: &str,
    revoked_ids: &HashSet<String>,
) -> Result<SnapshotBuildResult> {
    let episodes = read_episode_records(input)?;
    if episodes.is_empty() {
        bail!("no episode records found in {}", input.display());
    }

    let mut seen_hash = HashSet::new();
    let mut filtered = Vec::new();
    for ep in episodes {
        if revoked_ids.contains(&ep.id) {
            continue;
        }
        if !matches!(ep.license.as_str(), "CC0-1.0" | "CC-BY-4.0") {
            continue;
        }
        if !(ep.consent.public_searchable && ep.consent.trainable) {
            continue;
        }
        if seen_hash.insert(ep.content_hash.clone()) {
            filtered.push(ep);
        }
    }

    let out_dir = out_root.join(format!("dataset-{version}"));
    fs::create_dir_all(&out_dir)?;

    let train_path = out_dir.join("train.jsonl.zst");
    let val_path = out_dir.join("val.jsonl.zst");
    let mut train_writer = zstd::stream::write::Encoder::new(fs::File::create(&train_path)?, 3)?;
    let mut val_writer = zstd::stream::write::Encoder::new(fs::File::create(&val_path)?, 3)?;

    let mut train_count = 0usize;
    let mut val_count = 0usize;
    let mut license_breakdown: BTreeMap<String, usize> = BTreeMap::new();

    for ep in &filtered {
        *license_breakdown.entry(ep.license.clone()).or_default() += 1;
        let bucket = split_bucket(&ep.id, split_seed);
        let line = serde_json::to_string(ep)? + "\n";
        if bucket < 98 {
            train_writer.write_all(line.as_bytes())?;
            train_count += 1;
        } else {
            val_writer.write_all(line.as_bytes())?;
            val_count += 1;
        }
    }
    train_writer.finish()?;
    val_writer.finish()?;

    let sft_path = out_dir.join("sft.jsonl.zst");
    let tooltrace_path = out_dir.join("tooltrace.jsonl.zst");
    let mut sft_writer = zstd::stream::write::Encoder::new(fs::File::create(&sft_path)?, 3)?;
    let mut tt_writer = zstd::stream::write::Encoder::new(fs::File::create(&tooltrace_path)?, 3)?;
    for ep in &filtered {
        let sft = derive_sft(ep);
        let tt = derive_tooltrace(ep);
        sft_writer.write_all((serde_json::to_string(&sft)? + "\n").as_bytes())?;
        tt_writer.write_all((serde_json::to_string(&tt)? + "\n").as_bytes())?;
    }
    sft_writer.finish()?;
    tt_writer.finish()?;

    let manifest = serde_json::json!({
        "version": version,
        "total_records": filtered.len(),
        "train_count": train_count,
        "val_count": val_count,
        "split_rule": "blake3(id|seed)%100 < 98 => train",
        "license_breakdown": license_breakdown,
        "files": [
            "train.jsonl.zst",
            "val.jsonl.zst",
            "sft.jsonl.zst",
            "tooltrace.jsonl.zst"
        ]
    });
    let manifest_path = out_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    let manifest_hash = sha256_file(&manifest_path)?;

    let checksums = checksums(&[
        &train_path,
        &val_path,
        &sft_path,
        &tooltrace_path,
        &manifest_path,
    ])?;
    fs::write(out_dir.join("CHECKSUMS.txt"), checksums)?;

    let datacard = format!(
        "# DATA_CARD\n\nVersion: {version}\n\nTotal: {}\nTrain: {train_count}\nVal: {val_count}\n\nSanitized traces suitable for SFT and tool-use training.\n",
        filtered.len()
    );
    fs::write(out_dir.join("DATA_CARD.md"), datacard)?;

    Ok(SnapshotBuildResult {
        version: version.to_string(),
        train_count,
        val_count,
        out_dir,
        manifest_hash,
    })
}

pub async fn publish_snapshot(
    config: &AppConfig,
    version: &str,
    snapshot_path: &Path,
    dry_run: bool,
) -> Result<SnapshotPublishResult> {
    let snapshot_dir = resolve_snapshot_dir(version, snapshot_path)?;
    let required = required_snapshot_files(&snapshot_dir)?;
    let manifest_path = snapshot_dir.join("manifest.json");
    let checksums_path = snapshot_dir.join("CHECKSUMS.txt");
    let data_card_path = snapshot_dir.join("DATA_CARD.md");

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .context("invalid manifest.json")?;
    let checksums = fs::read_to_string(&checksums_path)
        .with_context(|| format!("failed to read {}", checksums_path.display()))?;
    let data_card = fs::read_to_string(&data_card_path)
        .with_context(|| format!("failed to read {}", data_card_path.display()))?;

    let mut file_entries = Vec::new();
    for path in required {
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("unknown")
            .to_string();
        let md = fs::metadata(&path)?;
        file_entries.push(SnapshotFileEntry {
            name,
            bytes: md.len(),
            sha256: sha256_file(&path)?,
        });
    }
    file_entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut object_prefix = None;
    if !dry_run {
        object_prefix = publish_snapshot_to_worker(
            config,
            version,
            &manifest,
            &checksums,
            &file_entries,
            &data_card,
        )
        .await?
        .object_prefix;

        index_snapshot_pointer(config, version, &manifest, object_prefix.as_deref()).await?;
    }

    Ok(SnapshotPublishResult {
        version: version.to_string(),
        snapshot_dir,
        object_prefix,
        indexed: !dry_run,
    })
}

fn split_bucket(id: &str, seed: &str) -> u8 {
    let value = format!("{id}|{seed}");
    let hash = blake3::hash(value.as_bytes());
    hash.as_bytes()[0] % 100
}

fn read_episode_records(path: &Path) -> Result<Vec<EpisodeRecord>> {
    let mut out = Vec::new();
    if path.is_file() {
        parse_episode_file(path, &mut out)?;
        return Ok(out);
    }

    if path.is_dir() {
        for entry in ignore::WalkBuilder::new(path)
            .hidden(false)
            .git_ignore(false)
            .build()
        {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            if entry.file_type().map(|f| f.is_file()).unwrap_or(false) {
                parse_episode_file(entry.path(), &mut out)?;
            }
        }
        return Ok(out);
    }

    Ok(out)
}

fn parse_episode_file(path: &Path, out: &mut Vec<EpisodeRecord>) -> Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(ep) = serde_json::from_str::<EpisodeRecord>(&line) {
            out.push(ep);
        }
    }
    Ok(())
}

fn checksums(paths: &[&Path]) -> Result<String> {
    let mut lines = Vec::new();
    for path in paths {
        let bytes = fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hasher.finalize();
        let digest_hex = format!("{:x}", digest);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        lines.push(format!("{}  {}", digest_hex, name));
    }
    lines.sort();
    Ok(lines.join("\n") + "\n")
}

fn resolve_snapshot_dir(version: &str, snapshot_path: &Path) -> Result<PathBuf> {
    let expected_name = format!("dataset-{version}");
    if snapshot_path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v == expected_name)
        .unwrap_or(false)
    {
        if snapshot_path.is_dir() {
            return Ok(snapshot_path.to_path_buf());
        }
        bail!(
            "snapshot path is not a directory: {}",
            snapshot_path.display()
        );
    }

    let candidate = snapshot_path.join(expected_name);
    if candidate.is_dir() {
        return Ok(candidate);
    }

    bail!(
        "snapshot directory not found: expected {} or {}",
        snapshot_path.display(),
        candidate.display()
    )
}

fn required_snapshot_files(snapshot_dir: &Path) -> Result<Vec<PathBuf>> {
    let names = [
        "train.jsonl.zst",
        "val.jsonl.zst",
        "sft.jsonl.zst",
        "tooltrace.jsonl.zst",
        "manifest.json",
        "CHECKSUMS.txt",
        "DATA_CARD.md",
    ];
    let mut out = Vec::new();
    for name in names {
        let path = snapshot_dir.join(name);
        if !path.is_file() {
            bail!("missing required snapshot artifact: {}", path.display());
        }
        out.push(path);
    }
    Ok(out)
}

async fn publish_snapshot_to_worker(
    config: &AppConfig,
    version: &str,
    manifest: &serde_json::Value,
    checksums: &str,
    files: &[SnapshotFileEntry],
    data_card: &str,
) -> Result<WorkerSnapshotPublishResponse> {
    let base_url = config
        .worker
        .base_url
        .as_ref()
        .context("missing TRACE_SHARE_WORKER_BASE_URL")?;
    let endpoint = format!("{}/v1/snapshots", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.worker.timeout_seconds.max(5),
        ))
        .build()?;

    let payload = serde_json::json!({
        "version": version,
        "created_at": Utc::now().to_rfc3339(),
        "manifest": manifest,
        "checksums": checksums,
        "files": files,
        "data_card": data_card,
    });

    let mut attempt: u32 = 0;
    loop {
        let mut req = client.post(&endpoint).json(&payload);
        if let Some(token) = config.worker.api_token.as_ref() {
            req = req.bearer_auth(token);
        }

        let res = req.send().await;
        match res {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(resp.json::<WorkerSnapshotPublishResponse>().await?);
                }
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!(
                        "worker snapshot publish failed: status={} body={}",
                        status,
                        body
                    );
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("worker snapshot publish request failed after retries");
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

async fn index_snapshot_pointer(
    config: &AppConfig,
    version: &str,
    manifest: &serde_json::Value,
    object_prefix: Option<&str>,
) -> Result<()> {
    let rest_url = config
        .upstash
        .rest_url
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_URL")?;
    let token = config
        .upstash
        .rest_token
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_TOKEN")?;
    let endpoint = format!("{}/upsert-data", rest_url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let payload = serde_json::json!({
        "vectors": [
            {
                "id": format!("snapshot:{version}"),
                "data": format!("trace-share dataset snapshot {version}"),
                "metadata": {
                    "kind": "dataset_snapshot",
                    "snapshot_version": version,
                    "total_records": manifest.get("total_records"),
                    "train_count": manifest.get("train_count"),
                    "val_count": manifest.get("val_count"),
                    "pointer": {
                        "storage": "r2",
                        "object_key": object_prefix,
                        "snapshot_version": version
                    }
                }
            }
        ]
    });

    let mut attempt: u32 = 0;
    loop {
        let res = client
            .post(&endpoint)
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await;
        match res {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(());
                }
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!(
                        "upstash snapshot pointer index failed: status={} body={}",
                        status,
                        body
                    );
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("upstash snapshot pointer request failed after retries");
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
