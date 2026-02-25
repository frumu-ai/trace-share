use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::{
    config::{AppConfig, ensure_dirs},
    consent::require_consent,
    episode::build_episodes,
    parser::{parse_jsonl_file_from_offset, parse_source_file},
    publish::index_episode_pointer,
    sanitize::{SanitizationReport, sanitize_events},
    sources::{SourceDef, discover_files, resolve_sources},
    state::{RunStats, StateStore},
    worker::upload_episode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOptions {
    pub sources: Vec<String>,
    pub dry_run: bool,
    pub review: bool,
    pub yes: bool,
    pub include_raw: bool,
    pub show_payload: bool,
    pub preview_limit: usize,
    pub explain_size: bool,
    pub export_payload_path: Option<PathBuf>,
    pub export_limit: Option<usize>,
    pub max_upload_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceSizeStats {
    pub source: String,
    pub scanned_files: usize,
    pub input_file_bytes: u64,
    pub parsed_event_text_bytes: u64,
    pub sanitized_event_text_bytes: u64,
    pub episode_payload_bytes: u64,
    pub would_upload_docs: usize,
    pub skipped_existing_docs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub scanned_files: usize,
    pub produced_docs: usize,     // episode count
    pub uploaded_docs: usize,     // uploaded episode count
    pub would_upload_docs: usize, // would-upload episode count
    pub skipped_existing_docs: usize,
    pub capped_docs: usize,
    pub redactions: usize,
    pub would_upload_bytes: u64,
    pub uploaded_bytes: u64,
    pub capped_bytes: u64,
    pub by_source: HashMap<String, usize>,
    pub payload_preview: Vec<crate::episode::EpisodeRecord>,
    pub source_size_stats: Vec<SourceSizeStats>,
    pub exported_payload_docs: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SourceCursor {
    files: HashMap<String, FileCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileCursor {
    last_byte_offset: u64,
    file_fingerprint: String,
}

pub async fn run_once(config: &AppConfig, options: &RunOptions) -> Result<RunResult> {
    if !options.dry_run && (!options.yes || !options.review) {
        anyhow::bail!("run uploads require --review and --yes (or use --dry-run)");
    }

    ensure_dirs()?;
    let store = StateStore::open_default()?;
    let consent = require_consent(&store)?;
    let run_id = Uuid::new_v4().to_string();
    store.start_run(&run_id)?;

    let selected_sources = select_sources(resolve_sources(config).await?, &options.sources);

    let mut scanned_files = 0usize;
    let mut produced_docs = 0usize;
    let mut uploaded_docs = 0usize;
    let mut would_upload_docs = 0usize;
    let mut skipped_existing_docs = 0usize;
    let mut capped_docs = 0usize;
    let mut redactions = 0usize;
    let mut would_upload_bytes = 0u64;
    let mut uploaded_bytes = 0u64;
    let mut capped_bytes = 0u64;
    let mut by_source = HashMap::new();
    let mut payload_preview = Vec::new();
    let mut source_size_stats = Vec::new();

    let mut export_writer = if let Some(path) = &options.export_payload_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Some(BufWriter::new(fs::File::create(path)?))
    } else {
        None
    };
    let export_limit = options.export_limit.unwrap_or(usize::MAX);
    let mut exported_payload_docs = 0usize;

    for source in selected_sources {
        let files = discover_files(&source)?;
        let mut source_docs = 0usize;
        let mut source_cursor = load_source_cursor(&store, &source.id)?;
        let mut source_stats = SourceSizeStats {
            source: source.id.clone(),
            ..Default::default()
        };

        for file in files {
            scanned_files += 1;
            source_stats.scanned_files += 1;
            let path_str = file.to_string_lossy().to_string();
            let fingerprint = file_fingerprint(&file)?;
            if let Ok(md) = fs::metadata(&file) {
                source_stats.input_file_bytes += md.len();
            }

            let prior = source_cursor.files.get(&path_str);
            let parsed: Result<(Vec<crate::models::CanonicalEvent>, u64)> =
                if source.format == "jsonl" {
                    let start_offset = prior
                        .filter(|c| c.file_fingerprint == fingerprint)
                        .map(|c| c.last_byte_offset)
                        .unwrap_or(0);
                    parse_jsonl_file_from_offset(&file, &source.id, start_offset)
                } else {
                    Ok((
                        parse_source_file(
                            &file,
                            &source.id,
                            &source.format,
                            source.parser_hint.as_deref(),
                        )?,
                        0,
                    ))
                };
            let (events, next_offset) = match parsed {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "[warn] source={} file={} parse failed: {}",
                        source.id,
                        file.display(),
                        e
                    );
                    continue;
                }
            };

            source_cursor.files.insert(
                path_str.clone(),
                FileCursor {
                    last_byte_offset: next_offset,
                    file_fingerprint: fingerprint.clone(),
                },
            );

            if events.is_empty() {
                store.upsert_file_fingerprint(&path_str, &fingerprint)?;
                continue;
            }
            source_stats.parsed_event_text_bytes +=
                events.iter().map(|e| e.text.len() as u64).sum::<u64>();

            let (sanitized, report): (_, SanitizationReport) = sanitize_events(&events);
            redactions += report.total_redactions;
            source_stats.sanitized_event_text_bytes +=
                sanitized.iter().map(|e| e.text.len() as u64).sum::<u64>();

            if options.review {
                print_review(&source.id, &file, &report);
            }

            let mut episodes = build_episodes(
                &source.id,
                &events[0].session_id,
                &sanitized,
                options.include_raw,
                &consent.accepted_at,
                &consent.consent_version,
                &consent.license,
                "policy-v1",
                "sanitizer-v1",
            );

            for episode in &mut episodes {
                episode.session_id = crate::publish::hash_identifier(
                    &crate::publish::load_or_create_anonymization_salt()?,
                    &episode.session_id,
                );
            }

            produced_docs += episodes.len();
            source_docs += episodes.len();

            for episode in episodes {
                if store.has_episode_upload(&episode.id)? {
                    skipped_existing_docs += 1;
                    source_stats.skipped_existing_docs += 1;
                    continue;
                }

                let episode_bytes = serde_json::to_vec(&episode)?.len() as u64;
                source_stats.episode_payload_bytes += episode_bytes;
                would_upload_docs += 1;
                source_stats.would_upload_docs += 1;
                would_upload_bytes += episode_bytes;

                if (options.dry_run || options.show_payload)
                    && payload_preview.len() < options.preview_limit
                {
                    payload_preview.push(episode.clone());
                }
                if let Some(writer) = export_writer.as_mut() {
                    if exported_payload_docs < export_limit {
                        serde_json::to_writer(&mut *writer, &episode)?;
                        writer.write_all(b"\n")?;
                        exported_payload_docs += 1;
                    }
                }

                if !options.dry_run {
                    if let Some(limit) = options.max_upload_bytes {
                        if limit > 0 && uploaded_bytes + episode_bytes > limit {
                            capped_docs += 1;
                            capped_bytes += episode_bytes;
                            continue;
                        }
                    }

                    let upload = upload_episode(config, &episode).await?;
                    index_episode_pointer(config, &episode, &upload.object_key, None).await?;
                    uploaded_docs += 1;
                    uploaded_bytes += episode_bytes;
                    store.upsert_episode_upload(
                        &episode.id,
                        &episode.content_hash,
                        &episode.source_tool,
                        &episode.session_id,
                        &upload.object_key,
                        &episode.consent.consent_version,
                        &episode.license,
                    )?;
                }
            }

            store.upsert_file_fingerprint(&path_str, &fingerprint)?;
        }

        let cursor_json = serde_json::to_string(&source_cursor)?;
        store.upsert_source_cursor(&source.id, &cursor_json)?;
        by_source.insert(source.id, source_docs);
        source_size_stats.push(source_stats);
    }

    if let Some(writer) = export_writer.as_mut() {
        writer.flush()?;
    }

    store.finish_run(&RunStats {
        run_id,
        scanned_files,
        produced_docs,
        uploaded_docs,
        redactions,
        errors: 0,
    })?;

    Ok(RunResult {
        scanned_files,
        produced_docs,
        uploaded_docs,
        would_upload_docs,
        skipped_existing_docs,
        capped_docs,
        redactions,
        would_upload_bytes,
        uploaded_bytes,
        capped_bytes,
        by_source,
        payload_preview,
        source_size_stats,
        exported_payload_docs,
    })
}

fn select_sources(all: Vec<SourceDef>, only: &[String]) -> Vec<SourceDef> {
    if only.is_empty() {
        return all;
    }
    all.into_iter()
        .filter(|s| only.iter().any(|x| x == &s.id))
        .collect()
}

fn file_fingerprint(path: &Path) -> Result<String> {
    let md = fs::metadata(path)?;
    let size = md.len();
    let modified = md
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let seed = format!("{}:{}", size, modified);
    Ok(blake3::hash(seed.as_bytes()).to_hex().to_string())
}

fn load_source_cursor(store: &StateStore, source_id: &str) -> Result<SourceCursor> {
    if let Some(raw) = store.source_cursor(source_id)? {
        if raw.trim().is_empty() {
            return Ok(SourceCursor::default());
        }
        if let Ok(cursor) = serde_json::from_str::<SourceCursor>(&raw) {
            return Ok(cursor);
        }
    }
    Ok(SourceCursor::default())
}

fn print_review(source_id: &str, path: &Path, report: &SanitizationReport) {
    println!("[review] source={source_id} file={}", path.display());
    println!(
        "[review] redactions total={} secrets={} email={} ip={} path={}",
        report.total_redactions,
        report.secret_redactions,
        report.email_redactions,
        report.ip_redactions,
        report.path_redactions,
    );
    for (idx, sample) in report.sample_redacted.iter().enumerate() {
        println!("[review][sample:{}] {}", idx + 1, sample);
    }
}
