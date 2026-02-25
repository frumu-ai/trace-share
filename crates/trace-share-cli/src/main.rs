use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use std::{collections::HashSet, path::PathBuf};
use trace_share_core::{
    config::{ensure_dirs, load_config},
    consent::init_consent,
    pipeline::{RunOptions, run_once},
    revocation::{revoke_local, sync_revocations},
    snapshot::{build_snapshot, publish_snapshot},
    sources::{SourceDef, add_local_source, discover_files, resolve_sources},
    split_pipeline::{publish_from_input, sanitize_to_dir, scan_to_dir},
    state::StateStore,
};

#[derive(Debug, Parser)]
#[command(name = "trace-share")]
#[command(about = "Sanitize and share coding-agent traces safely")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(RunCmd),
    Sources(SourcesCmd),
    Consent(ConsentCmd),
    Revoke(RevokeCmd),
    Snapshot(SnapshotCmd),
    Scan(ScanCmd),
    Sanitize(IOCmd),
    Publish(PublishCmd),
    Status,
    Reset(ResetCmd),
}

#[derive(Debug, Args)]
struct RunCmd {
    #[arg(long = "source")]
    sources: Vec<String>,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    review: bool,
    #[arg(long)]
    yes: bool,
    #[arg(
        long,
        help = "Include raw trace content in uploads (disabled by default)"
    )]
    include_raw: bool,
    #[arg(long)]
    show_payload: bool,
    #[arg(long, default_value_t = 10)]
    preview_limit: usize,
    #[arg(long, help = "Print per-source size breakdown")]
    explain_size: bool,
    #[arg(long, help = "Write sanitized would-upload episodes to JSONL")]
    export_payload: Option<String>,
    #[arg(
        long,
        help = "Max records to write with --export-payload (default: unlimited)"
    )]
    export_limit: Option<usize>,
    #[arg(long)]
    max_upload_bytes: Option<u64>,
}

#[derive(Debug, Args)]
struct SourcesCmd {
    #[command(subcommand)]
    command: SourcesSubcommand,
}

#[derive(Debug, Args)]
struct ConsentCmd {
    #[command(subcommand)]
    command: ConsentSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConsentSubcommand {
    Init(ConsentInitCmd),
    Status,
}

#[derive(Debug, Args)]
struct ConsentInitCmd {
    #[arg(long)]
    license: String,
    #[arg(long, default_value = "2026-02-25")]
    consent_version: String,
}

#[derive(Debug, Args)]
struct RevokeCmd {
    #[command(subcommand)]
    command: RevokeSubcommand,
}

#[derive(Debug, Subcommand)]
enum RevokeSubcommand {
    Add(RevokeAddCmd),
    Sync,
}

#[derive(Debug, Args)]
struct RevokeAddCmd {
    #[arg(long)]
    episode_id: String,
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Debug, Args)]
struct SnapshotCmd {
    #[command(subcommand)]
    command: SnapshotSubcommand,
}

#[derive(Debug, Subcommand)]
enum SnapshotSubcommand {
    Build(SnapshotBuildCmd),
    Publish(SnapshotPublishCmd),
}

#[derive(Debug, Args)]
struct SnapshotBuildCmd {
    #[arg(long)]
    version: String,
    #[arg(long = "in")]
    input: String,
    #[arg(long = "out", default_value = ".")]
    out: String,
    #[arg(long, default_value = "trace-share-split-v1")]
    split_seed: String,
}

#[derive(Debug, Args)]
struct SnapshotPublishCmd {
    #[arg(long)]
    version: String,
    #[arg(long = "from", default_value = ".")]
    from: String,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Subcommand)]
enum SourcesSubcommand {
    Detect,
    List,
    Add(AddSourceCmd),
    Update,
}

#[derive(Debug, Args)]
struct AddSourceCmd {
    #[arg(long)]
    name: String,
    #[arg(long)]
    root: String,
    #[arg(long)]
    glob: String,
    #[arg(long)]
    format: String,
    #[arg(long)]
    parser: Option<String>,
}

#[derive(Debug, Args)]
struct ScanCmd {
    #[arg(long = "in")]
    input: String,
    #[arg(long = "out")]
    out: String,
}

#[derive(Debug, Args)]
struct IOCmd {
    #[arg(long = "in")]
    input: String,
    #[arg(long = "out")]
    out: String,
    #[arg(long)]
    policy: Option<String>,
}

#[derive(Debug, Args)]
struct PublishCmd {
    #[arg(long = "in")]
    input: String,
    #[arg(long)]
    namespace: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    review: bool,
    #[arg(long)]
    yes: bool,
    #[arg(
        long,
        help = "Include raw trace content in uploads (disabled by default)"
    )]
    include_raw: bool,
    #[arg(long)]
    max_upload_bytes: Option<u64>,
}

#[derive(Debug, Args)]
struct ResetCmd {
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    yes: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    ensure_dirs()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(cmd) => run_command(cmd).await,
        Commands::Sources(cmd) => sources_command(cmd).await,
        Commands::Consent(cmd) => consent_command(cmd),
        Commands::Revoke(cmd) => revoke_command(cmd).await,
        Commands::Snapshot(cmd) => snapshot_command(cmd).await,
        Commands::Scan(cmd) => scan_command(cmd),
        Commands::Sanitize(cmd) => sanitize_command(cmd),
        Commands::Publish(cmd) => publish_command(cmd).await,
        Commands::Status => status_command(),
        Commands::Reset(cmd) => reset_command(cmd),
    }
}

async fn run_command(cmd: RunCmd) -> Result<()> {
    if !cmd.dry_run && (!cmd.yes || !cmd.review) {
        bail!("run uploads require --review and --yes (or use --dry-run)");
    }

    if let Some(since) = &cmd.since {
        println!("since filter acknowledged (MVP placeholder): {since}");
    }

    let cfg = load_config()?;
    let include_raw = cmd.include_raw || !cfg.policy.allowlist_mode;
    let result = run_once(
        &cfg,
        &RunOptions {
            sources: cmd.sources,
            dry_run: cmd.dry_run,
            review: cmd.review,
            yes: cmd.yes,
            include_raw,
            show_payload: cmd.show_payload,
            preview_limit: cmd.preview_limit.max(1),
            explain_size: cmd.explain_size,
            export_payload_path: cmd.export_payload.as_ref().map(PathBuf::from),
            export_limit: cmd.export_limit,
            max_upload_bytes: cmd.max_upload_bytes,
        },
    )
    .await?;

    println!("run complete");
    println!("scanned_files={}", result.scanned_files);
    println!("produced_docs={}", result.produced_docs);
    println!("uploaded_docs={}", result.uploaded_docs);
    println!("would_upload_docs={}", result.would_upload_docs);
    println!("skipped_existing_docs={}", result.skipped_existing_docs);
    println!("capped_docs={}", result.capped_docs);
    println!(
        "would_upload_bytes={} ({})",
        result.would_upload_bytes,
        format_bytes(result.would_upload_bytes)
    );
    println!(
        "uploaded_bytes={} ({})",
        result.uploaded_bytes,
        format_bytes(result.uploaded_bytes)
    );
    println!(
        "capped_bytes={} ({})",
        result.capped_bytes,
        format_bytes(result.capped_bytes)
    );
    println!("redactions={}", result.redactions);
    for (source, count) in &result.by_source {
        println!("source={} docs={}", source, count);
    }
    if cmd.explain_size {
        print_size_breakdown(&result);
    }
    if let Some(path) = &cmd.export_payload {
        println!(
            "export_payload_file={} exported_payload_docs={}",
            path, result.exported_payload_docs
        );
    }
    if cmd.dry_run && !result.payload_preview.is_empty() {
        if cmd.show_payload {
            println!("{}", serde_json::to_string_pretty(&result.payload_preview)?);
        } else {
            println!(
                "payload preview available: rerun with --show-payload to print {} docs",
                result.payload_preview.len()
            );
        }
    }

    Ok(())
}

async fn sources_command(cmd: SourcesCmd) -> Result<()> {
    let cfg = load_config()?;
    match cmd.command {
        SourcesSubcommand::Detect => {
            let sources = resolve_sources(&cfg).await?;
            for source in sources {
                let files = discover_files(&source)?;
                println!("{} => {} files", source.id, files.len());
            }
        }
        SourcesSubcommand::List => {
            let sources = resolve_sources(&cfg).await?;
            for source in sources {
                print_source(&source);
            }
        }
        SourcesSubcommand::Add(cmd) => {
            let source = SourceDef {
                id: cmd.name.clone(),
                display_name: Some(cmd.name),
                roots: vec![cmd.root],
                globs: vec![cmd.glob],
                format: cmd.format,
                parser_hint: cmd.parser,
                platforms: None,
                requires_opt_in: Some(true),
            };
            let path = add_local_source(&cfg, source)?;
            println!("source added to {}", path.display());
        }
        SourcesSubcommand::Update => {
            if !cfg.remote_registry.enabled {
                println!("remote registry disabled in config");
            } else {
                let _ = trace_share_core::sources::load_remote_registry(&cfg).await?;
                println!("remote registry cache refreshed");
            }
        }
    }
    Ok(())
}

fn consent_command(cmd: ConsentCmd) -> Result<()> {
    let state = StateStore::open_default()?;
    match cmd.command {
        ConsentSubcommand::Init(cmd) => {
            let consent = init_consent(&state, &cmd.license, &cmd.consent_version)?;
            println!("consent initialized");
            println!("accepted_at={}", consent.accepted_at);
            println!("license={}", consent.license);
            println!("consent_version={}", consent.consent_version);
        }
        ConsentSubcommand::Status => match state.consent_state()? {
            Some(consent) => {
                println!("consent configured");
                println!("accepted_at={}", consent.accepted_at);
                println!("license={}", consent.license);
                println!("consent_version={}", consent.consent_version);
                println!("public_searchable={}", consent.public_searchable);
                println!("trainable={}", consent.trainable);
            }
            None => println!("consent not initialized"),
        },
    }
    Ok(())
}

async fn revoke_command(cmd: RevokeCmd) -> Result<()> {
    let state = StateStore::open_default()?;
    match cmd.command {
        RevokeSubcommand::Add(cmd) => {
            revoke_local(&state, &cmd.episode_id, cmd.reason.as_deref())?;
            println!("revocation queued");
            println!("episode_id={}", cmd.episode_id);
        }
        RevokeSubcommand::Sync => {
            let cfg = load_config()?;
            let pushed = sync_revocations(&cfg, &state).await?;
            println!("revocations synced={pushed}");
        }
    }
    Ok(())
}

async fn snapshot_command(cmd: SnapshotCmd) -> Result<()> {
    let state = StateStore::open_default()?;
    match cmd.command {
        SnapshotSubcommand::Build(cmd) => {
            let revoked = state.all_revoked_ids()?.into_iter().collect::<HashSet<_>>();
            let result = build_snapshot(
                &cmd.version,
                &PathBuf::from(cmd.input),
                &PathBuf::from(cmd.out),
                &cmd.split_seed,
                &revoked,
            )?;
            println!("snapshot built");
            println!("version={}", result.version);
            println!("train_count={}", result.train_count);
            println!("val_count={}", result.val_count);
            println!("out_dir={}", result.out_dir.display());
            println!("manifest_hash={}", result.manifest_hash);
            state.record_snapshot(
                &result.version,
                result.train_count,
                result.val_count,
                &result.manifest_hash,
            )?;
        }
        SnapshotSubcommand::Publish(cmd) => {
            if !cmd.dry_run && !cmd.yes {
                bail!("snapshot publish requires --yes (or use --dry-run)");
            }

            let cfg = load_config()?;
            let result =
                publish_snapshot(&cfg, &cmd.version, &PathBuf::from(cmd.from), cmd.dry_run).await?;
            println!("snapshot publish complete");
            println!("version={}", result.version);
            println!("snapshot_dir={}", result.snapshot_dir.display());
            println!(
                "object_prefix={}",
                result.object_prefix.as_deref().unwrap_or("none")
            );
            println!("indexed={}", result.indexed);
            if !cmd.dry_run {
                state.mark_snapshot_published(&result.version)?;
            }
        }
    }
    Ok(())
}

fn print_source(source: &SourceDef) {
    println!("id={}", source.id);
    println!("  format={}", source.format);
    println!("  roots={}", source.roots.join(", "));
    println!("  globs={}", source.globs.join(", "));
    if let Some(parser) = &source.parser_hint {
        println!("  parser_hint={parser}");
    }
}

fn status_command() -> Result<()> {
    let state = StateStore::open_default()?;
    if let Some(consent) = state.consent_state()? {
        println!(
            "consent=initialized license={} version={}",
            consent.license, consent.consent_version
        );
    } else {
        println!("consent=missing");
    }

    let pending_revocations = state.pending_revocations()?.len();
    println!("pending_revocations={pending_revocations}");

    let totals = state.episode_totals_by_source()?;
    if totals.is_empty() {
        println!("no episode uploads yet");
    } else {
        for (source, count) in totals {
            println!("{} => {} episodes", source, count);
        }
    }
    Ok(())
}

fn reset_command(cmd: ResetCmd) -> Result<()> {
    if !cmd.yes {
        bail!("reset requires --yes");
    }

    let state = StateStore::open_default()?;
    if cmd.all {
        state.reset_all()?;
        println!("reset all sources + uploads");
        return Ok(());
    }

    if let Some(source) = cmd.source {
        state.reset_source(&source)?;
        println!("reset source={source}");
        return Ok(());
    }

    bail!("provide --all or --source <name>")
}

fn scan_command(cmd: ScanCmd) -> Result<()> {
    let out_dir = PathBuf::from(cmd.out);
    let result = scan_to_dir(&cmd.input, &out_dir)?;
    println!("scan complete");
    println!("input_files={}", result.input_files);
    println!("produced_events={}", result.produced_events);
    println!("output_file={}", result.output_file.display());
    Ok(())
}

fn sanitize_command(cmd: IOCmd) -> Result<()> {
    let input = PathBuf::from(cmd.input);
    let out_dir = PathBuf::from(cmd.out);
    let policy = cmd.policy.as_deref().map(PathBuf::from);
    let result = sanitize_to_dir(&input, &out_dir, policy.as_deref())?;
    println!("sanitize complete");
    println!("input_events={}", result.input_events);
    println!("output_events={}", result.output_events);
    println!("output_file={}", result.output_file.display());
    println!("report_file={}", result.report_file.display());
    println!("redactions={}", result.report.total_redactions);
    Ok(())
}

async fn publish_command(cmd: PublishCmd) -> Result<()> {
    let cfg = load_config()?;
    let include_raw = cmd.include_raw || !cfg.policy.allowlist_mode;
    let input = PathBuf::from(cmd.input);
    let result = publish_from_input(
        &cfg,
        &input,
        cmd.namespace.as_deref(),
        cmd.dry_run,
        cmd.review,
        cmd.yes,
        include_raw,
        cmd.max_upload_bytes,
    )
    .await?;
    println!("publish complete");
    println!("produced_docs={}", result.produced_docs);
    println!("would_upload_docs={}", result.would_upload_docs);
    println!("uploaded_docs={}", result.uploaded_docs);
    println!("skipped_existing_docs={}", result.skipped_existing_docs);
    println!("capped_docs={}", result.capped_docs);
    println!(
        "would_upload_bytes={} ({})",
        result.would_upload_bytes,
        format_bytes(result.would_upload_bytes)
    );
    println!(
        "uploaded_bytes={} ({})",
        result.uploaded_bytes,
        format_bytes(result.uploaded_bytes)
    );
    println!(
        "capped_bytes={} ({})",
        result.capped_bytes,
        format_bytes(result.capped_bytes)
    );
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn print_size_breakdown(result: &trace_share_core::pipeline::RunResult) {
    println!("size_breakdown:");
    for stats in &result.source_size_stats {
        println!(
            "source={} files={} input_bytes={} ({}) parsed_text_bytes={} ({}) sanitized_text_bytes={} ({}) payload_bytes={} ({}) would_upload_docs={} skipped_existing_docs={}",
            stats.source,
            stats.scanned_files,
            stats.input_file_bytes,
            format_bytes(stats.input_file_bytes),
            stats.parsed_event_text_bytes,
            format_bytes(stats.parsed_event_text_bytes),
            stats.sanitized_event_text_bytes,
            format_bytes(stats.sanitized_event_text_bytes),
            stats.episode_payload_bytes,
            format_bytes(stats.episode_payload_bytes),
            stats.would_upload_docs,
            stats.skipped_existing_docs
        );
    }
}
