use anyhow::{Context, Result, bail};
use glob::glob;
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::{
    config::{AppConfig, ensure_dirs},
    consent::require_consent,
    episode::{EpisodeRecord, build_episodes},
    models::CanonicalEvent,
    parser::parse_jsonl_file,
    publish::index_episode_pointer,
    sanitize::{SanitizationReport, sanitize_events},
    state::StateStore,
    worker::upload_episode,
};

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub input_files: usize,
    pub produced_events: usize,
    pub output_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SanitizeResult {
    pub input_events: usize,
    pub output_events: usize,
    pub output_file: PathBuf,
    pub report_file: PathBuf,
    pub report: SanitizationReport,
}

#[derive(Debug, Clone)]
pub struct PublishResult {
    pub produced_docs: usize,
    pub would_upload_docs: usize,
    pub uploaded_docs: usize,
    pub skipped_existing_docs: usize,
    pub capped_docs: usize,
    pub would_upload_bytes: u64,
    pub uploaded_bytes: u64,
    pub capped_bytes: u64,
}

pub fn scan_to_dir(input: &str, out_dir: &Path) -> Result<ScanResult> {
    ensure_dirs()?;
    fs::create_dir_all(out_dir)?;

    let input_files = collect_input_files(input)?;
    if input_files.is_empty() {
        bail!("no files found for input: {input}");
    }

    let output_file = out_dir.join("canonical_events.jsonl");
    let mut writer = BufWriter::new(fs::File::create(&output_file)?);

    let mut produced_events = 0usize;
    for path in &input_files {
        let events = parse_jsonl_file(path, "manual_scan")?;
        for event in events {
            serde_json::to_writer(&mut writer, &event)?;
            writer.write_all(b"\n")?;
            produced_events += 1;
        }
    }
    writer.flush()?;

    let summary = serde_json::json!({
        "input_files": input_files.len(),
        "produced_events": produced_events,
        "output_file": output_file,
    });
    fs::write(
        out_dir.join("scan_summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;

    Ok(ScanResult {
        input_files: input_files.len(),
        produced_events,
        output_file,
    })
}

pub fn sanitize_to_dir(
    input: &Path,
    out_dir: &Path,
    _policy: Option<&Path>,
) -> Result<SanitizeResult> {
    ensure_dirs()?;
    fs::create_dir_all(out_dir)?;

    let events = read_canonical_events(input)?;
    let input_events = events.len();
    let (sanitized, report) = sanitize_events(&events);

    let output_file = out_dir.join("sanitized_events.jsonl");
    let mut writer = BufWriter::new(fs::File::create(&output_file)?);
    for event in &sanitized {
        serde_json::to_writer(&mut writer, event)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;

    let report_file = out_dir.join("redaction_report.json");
    fs::write(&report_file, serde_json::to_vec_pretty(&report)?)?;

    Ok(SanitizeResult {
        input_events,
        output_events: sanitized.len(),
        output_file,
        report_file,
        report,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_from_input(
    config: &AppConfig,
    input: &Path,
    namespace: Option<&str>,
    dry_run: bool,
    review: bool,
    yes: bool,
    include_raw: bool,
    max_upload_bytes: Option<u64>,
) -> Result<PublishResult> {
    if !dry_run && (!yes || !review) {
        bail!("publish requires --review and --yes unless --dry-run");
    }

    ensure_dirs()?;
    let store = StateStore::open_default()?;

    let mut episodes = read_episode_records(input)?;
    if episodes.is_empty() {
        let consent = require_consent(&store)?;
        let events = read_canonical_events(input)?;
        if !events.is_empty() {
            let built = build_episodes(
                "manual_scan",
                &events[0].session_id,
                &events,
                include_raw,
                &consent.accepted_at,
                &consent.consent_version,
                &consent.license,
                "policy-v1",
                "sanitizer-v1",
            );
            episodes.extend(built);
        }
    }

    if let Some(ns) = namespace {
        for ep in &mut episodes {
            ep.source_tool = format!("{ns}:{}", ep.source_tool);
        }
    }

    let produced_docs = episodes.len();
    let mut would_upload_docs = 0usize;
    let mut uploaded_docs = 0usize;
    let mut skipped_existing_docs = 0usize;
    let mut capped_docs = 0usize;
    let mut would_upload_bytes = 0u64;
    let mut uploaded_bytes = 0u64;
    let mut capped_bytes = 0u64;

    for (idx, episode) in episodes.iter().enumerate() {
        if store.has_episode_upload(&episode.id)? {
            skipped_existing_docs += 1;
            continue;
        }
        let episode_bytes = serde_json::to_vec(episode)?.len() as u64;
        would_upload_docs += 1;
        would_upload_bytes += episode_bytes;

        if review && idx < 5 {
            println!(
                "[review] episode_id={} source_tool={} ts_start={}",
                episode.id, episode.source_tool, episode.ts_start
            );
            let preview = if episode.result.len() > 240 {
                format!("{}...", &episode.result[..240])
            } else {
                episode.result.clone()
            };
            println!("[review] text_preview={}", preview.replace('\n', " "));
        }

        if dry_run {
            continue;
        }

        if let Some(limit) = max_upload_bytes {
            if limit > 0 && uploaded_bytes + episode_bytes > limit {
                capped_docs += 1;
                capped_bytes += episode_bytes;
                continue;
            }
        }

        let upload = upload_episode(config, episode).await?;
        index_episode_pointer(config, episode, &upload.object_key, None).await?;
        store.upsert_episode_upload(
            &episode.id,
            &episode.content_hash,
            &episode.source_tool,
            &episode.session_id,
            &upload.object_key,
            &episode.consent.consent_version,
            &episode.license,
        )?;
        uploaded_docs += 1;
        uploaded_bytes += episode_bytes;
    }

    Ok(PublishResult {
        produced_docs,
        would_upload_docs,
        uploaded_docs,
        skipped_existing_docs,
        capped_docs,
        would_upload_bytes,
        uploaded_bytes,
        capped_bytes,
    })
}

fn read_canonical_events(input: &Path) -> Result<Vec<CanonicalEvent>> {
    let files = collect_files_from_path(input)?;
    let mut out = Vec::new();
    for path in files {
        let file = fs::File::open(&path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<CanonicalEvent>(&line) {
                out.push(event);
            }
        }
    }
    Ok(out)
}

fn read_episode_records(input: &Path) -> Result<Vec<EpisodeRecord>> {
    let files = collect_files_from_path(input)?;
    let mut out = Vec::new();
    for path in files {
        let file = fs::File::open(&path)?;
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
    }
    Ok(out)
}

fn collect_input_files(input: &str) -> Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();

    let has_glob = input.contains('*') || input.contains('?') || input.contains('[');
    if has_glob {
        for path in glob(input)
            .with_context(|| format!("invalid input glob: {input}"))?
            .flatten()
        {
            if path.is_file() {
                files.insert(path);
            }
        }
        return Ok(files.into_iter().collect());
    }

    let path = PathBuf::from(input);
    if path.is_file() {
        files.insert(path);
    } else if path.is_dir() {
        for entry in ignore::WalkBuilder::new(&path)
            .hidden(false)
            .git_ignore(false)
            .build()
        {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                files.insert(entry.path().to_path_buf());
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn collect_files_from_path(input: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if input.is_file() {
        files.push(input.to_path_buf());
        return Ok(files);
    }

    if input.is_dir() {
        for entry in ignore::WalkBuilder::new(input)
            .hidden(false)
            .git_ignore(false)
            .build()
        {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();
        files.dedup();
        return Ok(files);
    }

    bail!("input path does not exist: {}", input.display())
}
