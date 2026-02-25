use anyhow::{Context, Result};
use rand::{Rng, thread_rng};
use serde_json::json;
use std::fs;
use tokio::time::{Duration, sleep};

use crate::{
    config::{AppConfig, data_dir, validate_network_url, write_private_file},
    episode::EpisodeRecord,
    models::ChunkDocument,
    sanitize::contains_sensitive_patterns,
};

pub async fn publish_upsert_data(config: &AppConfig, docs: &[ChunkDocument]) -> Result<()> {
    if docs.is_empty() {
        return Ok(());
    }

    let anon_salt = load_or_create_anonymization_salt()?;
    let anonymized_docs = docs
        .iter()
        .map(|doc| anonymize_doc(doc, &anon_salt))
        .collect::<Vec<_>>();

    for doc in &anonymized_docs {
        if contains_sensitive_patterns(&doc.text) {
            anyhow::bail!(
                "refusing upload: unsanitized sensitive pattern detected in doc_id={}",
                doc.id
            );
        }
        if contains_sensitive_patterns(&serde_json::to_string(&doc.metadata).unwrap_or_default()) {
            anyhow::bail!(
                "refusing upload: sensitive pattern detected in metadata for doc_id={}",
                doc.id
            );
        }
    }

    let rest_url = config
        .upstash
        .rest_url
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_URL")?;
    validate_network_url(rest_url, "Upstash REST")?;
    let token = config
        .upstash
        .rest_token
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_TOKEN")?;

    let endpoint = format!("{}/upsert-data", rest_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().no_proxy().build()?;

    let payload = json!({
        "vectors": anonymized_docs.iter().map(|doc| {
            json!({
                "id": doc.id,
                "data": doc.text,
                "metadata": doc.metadata,
            })
        }).collect::<Vec<_>>()
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
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!("upstash error: status={} body={}", status, body);
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("upstash request failed after retries");
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

fn anonymize_doc(doc: &ChunkDocument, salt: &str) -> ChunkDocument {
    let mut out = doc.clone();
    out.metadata.source = hash_identifier(salt, &out.metadata.source);
    out.metadata.session_id = hash_identifier(salt, &out.metadata.session_id);
    out.metadata.repo_fingerprint = out
        .metadata
        .repo_fingerprint
        .as_ref()
        .map(|v| hash_identifier(salt, v));
    out.metadata.tool_names = out
        .metadata
        .tool_names
        .iter()
        .map(|v| hash_identifier(salt, v))
        .collect();
    out
}

pub fn hash_identifier(salt: &str, value: &str) -> String {
    let seed = format!("{salt}|{value}");
    blake3::hash(seed.as_bytes()).to_hex().to_string()
}

pub fn load_or_create_anonymization_salt() -> Result<String> {
    let dir = data_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("anonymization_salt");
    if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let salt = raw.trim();
        if !salt.is_empty() {
            return Ok(salt.to_string());
        }
    }
    let salt = uuid::Uuid::new_v4().to_string();
    write_private_file(&path, salt.as_bytes())?;
    Ok(salt)
}

pub async fn index_episode_pointer(
    config: &AppConfig,
    episode: &EpisodeRecord,
    object_key: &str,
    snapshot_version: Option<&str>,
) -> Result<()> {
    let rest_url = config
        .upstash
        .rest_url
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_URL")?;
    validate_network_url(rest_url, "Upstash REST")?;
    let token = config
        .upstash
        .rest_token
        .as_ref()
        .context("missing UPSTASH_VECTOR_REST_TOKEN")?;

    let salt = load_or_create_anonymization_salt()?;
    let endpoint = format!("{}/upsert-data", rest_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().no_proxy().build()?;

    let metadata = serde_json::json!({
        "kind": "episode_pointer",
        "source_tool": hash_identifier(&salt, &episode.source_tool),
        "session_id": hash_identifier(&salt, &episode.session_id),
        "license": episode.license,
        "consent_version": episode.consent.consent_version,
        "public_searchable": episode.consent.public_searchable,
        "trainable": episode.consent.trainable,
        "tool_names": episode.meta.tool_names.iter().map(|t| hash_identifier(&salt, t)).collect::<Vec<_>>(),
        "error_types": episode.meta.error_types,
        "pointer": {
            "storage": "r2",
            "object_key": object_key,
            "snapshot_version": snapshot_version
        }
    });

    if contains_sensitive_patterns(&serde_json::to_string(&metadata).unwrap_or_default()) {
        anyhow::bail!("refusing index: sensitive pattern detected in episode metadata");
    }

    let payload = serde_json::json!({
        "vectors": [
            {
                "id": episode.id,
                "data": episode.prompt,
                "metadata": metadata
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
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if !should_retry_status(status) || attempt >= 4 {
                    anyhow::bail!("upstash index error: status={} body={}", status, body);
                }
            }
            Err(e) => {
                let retryable_transport = e.is_timeout() || e.is_connect() || e.is_request();
                if !retryable_transport || attempt >= 4 {
                    return Err(e).context("upstash index request failed after retries");
                }
            }
        }

        attempt += 1;
        let jitter: u64 = thread_rng().gen_range(50..300);
        let wait_ms = (2u64.pow(attempt) * 200) + jitter;
        sleep(Duration::from_millis(wait_ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{hash_identifier, should_retry_status};

    #[test]
    fn retry_status_logic() {
        assert!(should_retry_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(should_retry_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!should_retry_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!should_retry_status(reqwest::StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn hash_identifier_is_stable() {
        let a = hash_identifier("salt", "session-a");
        let b = hash_identifier("salt", "session-a");
        let c = hash_identifier("salt", "session-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
