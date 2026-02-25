use std::{
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
};

use trace_share_core::episode::{
    EpisodeConsent, EpisodeMeta, EpisodeOutcome, EpisodeOutcomeSignals, EpisodeRecord,
};
use trace_share_core::snapshot::build_snapshot;

#[test]
fn snapshot_filters_revoked_invalid_and_deduped_records() {
    let root = std::env::temp_dir().join(format!("trace-share-snapshot-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let input = root.join("episodes.jsonl");

    let good = sample_episode("ep-good", "hash-shared", "CC0-1.0", true, true);
    let dup_hash = sample_episode("ep-dup", "hash-shared", "CC0-1.0", true, true);
    let revoked = sample_episode("ep-revoked", "hash-revoked", "CC0-1.0", true, true);
    let bad_license = sample_episode("ep-bad-license", "hash-license", "Apache-2.0", true, true);
    let not_trainable = sample_episode("ep-not-train", "hash-train", "CC0-1.0", true, false);

    write_jsonl(
        &input,
        &[good, dup_hash, revoked, bad_license, not_trainable],
    );

    let mut revoked_ids = HashSet::new();
    revoked_ids.insert("ep-revoked".to_string());

    let out = root.join("out");
    let result = build_snapshot("0.1.0", &input, &out, "seed-a", &revoked_ids)
        .expect("snapshot should build");

    assert_eq!(result.train_count + result.val_count, 1);

    let out_dir = out.join("dataset-0.1.0");
    assert!(out_dir.join("manifest.json").exists());
    assert!(out_dir.join("CHECKSUMS.txt").exists());
    assert!(out_dir.join("DATA_CARD.md").exists());
    assert!(out_dir.join("train.jsonl.zst").exists());
    assert!(out_dir.join("val.jsonl.zst").exists());
    assert!(out_dir.join("sft.jsonl.zst").exists());
    assert!(out_dir.join("tooltrace.jsonl.zst").exists());

    let train = read_zstd_jsonl(&out_dir.join("train.jsonl.zst"));
    let val = read_zstd_jsonl(&out_dir.join("val.jsonl.zst"));
    let combined = [train, val].concat();

    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0]["id"], "ep-good");
}

#[test]
fn snapshot_split_is_deterministic_for_same_seed() {
    let root = std::env::temp_dir().join(format!("trace-share-snapshot-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let input = root.join("episodes.jsonl");
    let out = root.join("out");

    let episodes = (0..40)
        .map(|i| {
            sample_episode(
                &format!("ep-{i}"),
                &format!("hash-{i}"),
                "CC-BY-4.0",
                true,
                true,
            )
        })
        .collect::<Vec<_>>();
    write_jsonl(&input, &episodes);

    let revoked = HashSet::new();
    let first =
        build_snapshot("det-a", &input, &out, "seed-fixed", &revoked).expect("first snapshot");
    let second =
        build_snapshot("det-b", &input, &out, "seed-fixed", &revoked).expect("second snapshot");

    assert_eq!(first.train_count, second.train_count);
    assert_eq!(first.val_count, second.val_count);

    let a_train = fs::read(out.join("dataset-det-a/train.jsonl.zst")).expect("read train a");
    let b_train = fs::read(out.join("dataset-det-b/train.jsonl.zst")).expect("read train b");
    let a_val = fs::read(out.join("dataset-det-a/val.jsonl.zst")).expect("read val a");
    let b_val = fs::read(out.join("dataset-det-b/val.jsonl.zst")).expect("read val b");
    assert_eq!(a_train, b_train);
    assert_eq!(a_val, b_val);
}

fn sample_episode(
    id: &str,
    content_hash: &str,
    license: &str,
    public_searchable: bool,
    trainable: bool,
) -> EpisodeRecord {
    EpisodeRecord {
        id: id.to_string(),
        source_tool: "codex".to_string(),
        session_id: format!("session-{id}"),
        ts_start: "2026-02-25T00:00:00Z".to_string(),
        ts_end: "2026-02-25T00:01:00Z".to_string(),
        prompt: format!("prompt-{id}"),
        context: "context".to_string(),
        trace: vec![],
        result: "result".to_string(),
        outcome: EpisodeOutcome {
            success: true,
            signals: EpisodeOutcomeSignals {
                tests_passed: Some(true),
                exit_code: Some(0),
                lint_fixed: Some(true),
            },
        },
        meta: EpisodeMeta {
            lang: Some("rust".to_string()),
            tool_names: vec!["shell".to_string()],
            error_types: vec![],
            repo_fingerprint: Some("repo".to_string()),
            os: Some("linux".to_string()),
            editor: Some("vscode".to_string()),
            raw_content_included: false,
        },
        consent: EpisodeConsent {
            accepted_at: "2026-02-25T00:00:00Z".to_string(),
            consent_version: "v1".to_string(),
            public_searchable,
            trainable,
        },
        license: license.to_string(),
        policy_version: "policy-v1".to_string(),
        sanitizer_version: "sanitizer-v1".to_string(),
        content_hash: content_hash.to_string(),
    }
}

fn write_jsonl(path: &Path, episodes: &[EpisodeRecord]) {
    let mut f = fs::File::create(path).expect("create jsonl");
    for ep in episodes {
        let line = serde_json::to_string(ep).expect("serialize episode");
        f.write_all(line.as_bytes()).expect("write line");
        f.write_all(b"\n").expect("write nl");
    }
}

fn read_zstd_jsonl(path: &Path) -> Vec<serde_json::Value> {
    let file = fs::File::open(path).expect("open zstd file");
    let decoder = zstd::stream::read::Decoder::new(file).expect("create decoder");
    let reader = BufReader::new(decoder);
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(&line).expect("parse json line"))
        .collect()
}
