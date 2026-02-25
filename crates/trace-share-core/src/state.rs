use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, params};
use std::{fs, path::PathBuf};

use crate::config::data_dir;
use crate::consent::ConsentState;

pub struct StateStore {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct RunStats {
    pub run_id: String,
    pub scanned_files: usize,
    pub produced_docs: usize,
    pub uploaded_docs: usize,
    pub redactions: usize,
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct RevocationRecord {
    pub episode_id: String,
    pub reason: Option<String>,
    pub revoked_at: String,
}

impl StateStore {
    pub fn open_default() -> Result<Self> {
        let dir = data_dir()?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("state.sqlite");
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sources (
              source_name TEXT PRIMARY KEY,
              last_scan_ts TEXT,
              cursor_json TEXT
            );

            CREATE TABLE IF NOT EXISTS files (
              path TEXT PRIMARY KEY,
              fingerprint TEXT,
              last_seen_ts TEXT
            );

            CREATE TABLE IF NOT EXISTS uploads (
              doc_id TEXT PRIMARY KEY,
              content_hash TEXT NOT NULL,
              source_name TEXT NOT NULL,
              session_id TEXT,
              ts_start TEXT,
              ts_end TEXT,
              uploaded_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS runs (
              run_id TEXT PRIMARY KEY,
              started_at TEXT,
              finished_at TEXT,
              scanned_files INT,
              produced_docs INT,
              uploaded_docs INT,
              redactions INT,
              errors INT
            );

            CREATE TABLE IF NOT EXISTS consent_state (
              id INTEGER PRIMARY KEY CHECK(id=1),
              accepted_at TEXT,
              consent_version TEXT,
              license TEXT,
              public_searchable INTEGER,
              trainable INTEGER,
              ack_sanitization INTEGER,
              ack_public_search INTEGER,
              ack_training_release INTEGER
            );

            CREATE TABLE IF NOT EXISTS episodes (
              id TEXT PRIMARY KEY,
              content_hash TEXT NOT NULL,
              source_tool TEXT NOT NULL,
              session_id_hash TEXT NOT NULL,
              r2_object_key TEXT,
              indexed_at TEXT,
              uploaded_at TEXT,
              consent_version TEXT,
              license TEXT
            );

            CREATE TABLE IF NOT EXISTS revocations (
              episode_id TEXT PRIMARY KEY,
              reason TEXT,
              revoked_at TEXT,
              pushed_at TEXT,
              push_status TEXT
            );

            CREATE TABLE IF NOT EXISTS snapshots (
              version TEXT PRIMARY KEY,
              built_at TEXT,
              train_count INT,
              val_count INT,
              manifest_hash TEXT,
              published_at TEXT
            );
            ",
        )?;
        Ok(())
    }

    pub fn has_upload(&self, doc_id: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM uploads WHERE doc_id = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![doc_id])?;
        Ok(rows.next()?.is_some())
    }

    pub fn insert_upload(
        &self,
        doc_id: &str,
        content_hash: &str,
        source_name: &str,
        session_id: &str,
        ts_start: &str,
        ts_end: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO uploads (doc_id, content_hash, source_name, session_id, ts_start, ts_end, uploaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                doc_id,
                content_hash,
                source_name,
                session_id,
                ts_start,
                ts_end,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_source_cursor(&self, source_name: &str, cursor_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sources (source_name, last_scan_ts, cursor_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(source_name) DO UPDATE SET
               last_scan_ts=excluded.last_scan_ts,
               cursor_json=excluded.cursor_json",
            params![source_name, Utc::now().to_rfc3339(), cursor_json],
        )?;
        Ok(())
    }

    pub fn source_cursor(&self, source_name: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT cursor_json FROM sources WHERE source_name = ?1")?;
        let mut rows = stmt.query(params![source_name])?;
        if let Some(row) = rows.next()? {
            let c: Option<String> = row.get(0)?;
            Ok(c)
        } else {
            Ok(None)
        }
    }

    pub fn upsert_file_fingerprint(&self, path: &str, fingerprint: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO files (path, fingerprint, last_seen_ts)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
               fingerprint=excluded.fingerprint,
               last_seen_ts=excluded.last_seen_ts",
            params![path, fingerprint, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn file_fingerprint(&self, path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT fingerprint FROM files WHERE path = ?1")?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            let fp: String = row.get(0)?;
            Ok(Some(fp))
        } else {
            Ok(None)
        }
    }

    pub fn start_run(&self, run_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (run_id, started_at) VALUES (?1, ?2)",
            params![run_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn finish_run(&self, stats: &RunStats) -> Result<()> {
        self.conn.execute(
            "UPDATE runs SET
               finished_at=?2,
               scanned_files=?3,
               produced_docs=?4,
               uploaded_docs=?5,
               redactions=?6,
               errors=?7
             WHERE run_id=?1",
            params![
                stats.run_id,
                Utc::now().to_rfc3339(),
                stats.scanned_files as i64,
                stats.produced_docs as i64,
                stats.uploaded_docs as i64,
                stats.redactions as i64,
                stats.errors as i64,
            ],
        )?;
        Ok(())
    }

    pub fn totals_by_source(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_name, COUNT(*) as c FROM uploads GROUP BY source_name ORDER BY source_name",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.flatten().collect())
    }

    pub fn episode_totals_by_source(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_tool, COUNT(*) as c FROM episodes GROUP BY source_tool ORDER BY source_tool",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.flatten().collect())
    }

    pub fn reset_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM sources", [])?;
        self.conn.execute("DELETE FROM files", [])?;
        self.conn.execute("DELETE FROM uploads", [])?;
        self.conn.execute("DELETE FROM consent_state", [])?;
        self.conn.execute("DELETE FROM episodes", [])?;
        self.conn.execute("DELETE FROM revocations", [])?;
        self.conn.execute("DELETE FROM snapshots", [])?;
        Ok(())
    }

    pub fn reset_source(&self, source_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sources WHERE source_name=?1",
            params![source_name],
        )?;
        self.conn.execute(
            "DELETE FROM uploads WHERE source_name=?1",
            params![source_name],
        )?;
        Ok(())
    }

    pub fn upsert_consent_state(&self, state: &ConsentState) -> Result<()> {
        self.conn.execute(
            "INSERT INTO consent_state (
                id, accepted_at, consent_version, license, public_searchable, trainable,
                ack_sanitization, ack_public_search, ack_training_release
            ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
              accepted_at=excluded.accepted_at,
              consent_version=excluded.consent_version,
              license=excluded.license,
              public_searchable=excluded.public_searchable,
              trainable=excluded.trainable,
              ack_sanitization=excluded.ack_sanitization,
              ack_public_search=excluded.ack_public_search,
              ack_training_release=excluded.ack_training_release",
            params![
                state.accepted_at,
                state.consent_version,
                state.license,
                state.public_searchable as i32,
                state.trainable as i32,
                state.ack_sanitization as i32,
                state.ack_public_search as i32,
                state.ack_training_release as i32,
            ],
        )?;
        Ok(())
    }

    pub fn consent_state(&self) -> Result<Option<ConsentState>> {
        let mut stmt = self.conn.prepare(
            "SELECT accepted_at, consent_version, license, public_searchable, trainable,
                    ack_sanitization, ack_public_search, ack_training_release
             FROM consent_state WHERE id=1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConsentState {
                accepted_at: row.get(0)?,
                consent_version: row.get(1)?,
                license: row.get(2)?,
                public_searchable: row.get::<_, i64>(3)? != 0,
                trainable: row.get::<_, i64>(4)? != 0,
                ack_sanitization: row.get::<_, i64>(5)? != 0,
                ack_public_search: row.get::<_, i64>(6)? != 0,
                ack_training_release: row.get::<_, i64>(7)? != 0,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_episode_upload(
        &self,
        id: &str,
        content_hash: &str,
        source_tool: &str,
        session_id_hash: &str,
        r2_object_key: &str,
        consent_version: &str,
        license: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO episodes (
                id, content_hash, source_tool, session_id_hash, r2_object_key,
                indexed_at, uploaded_at, consent_version, license
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
              r2_object_key=excluded.r2_object_key,
              indexed_at=excluded.indexed_at,
              uploaded_at=excluded.uploaded_at,
              consent_version=excluded.consent_version,
              license=excluded.license",
            params![
                id,
                content_hash,
                source_tool,
                session_id_hash,
                r2_object_key,
                Utc::now().to_rfc3339(),
                consent_version,
                license,
            ],
        )?;
        Ok(())
    }

    pub fn has_episode_upload(&self, id: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM episodes WHERE id=?1 LIMIT 1")?;
        let mut rows = stmt.query(params![id])?;
        Ok(rows.next()?.is_some())
    }

    pub fn upsert_revocation(
        &self,
        episode_id: &str,
        reason: Option<&str>,
        revoked_at: &str,
        push_status: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO revocations (episode_id, reason, revoked_at, pushed_at, push_status)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(episode_id) DO UPDATE SET
               reason=excluded.reason,
               revoked_at=excluded.revoked_at,
               push_status=excluded.push_status",
            params![episode_id, reason, revoked_at, push_status],
        )?;
        Ok(())
    }

    pub fn pending_revocations(&self) -> Result<Vec<RevocationRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT episode_id, reason, revoked_at
             FROM revocations
             WHERE push_status IS NULL OR push_status != 'pushed'
             ORDER BY revoked_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RevocationRecord {
                episode_id: row.get(0)?,
                reason: row.get(1)?,
                revoked_at: row.get(2)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn mark_revocation_pushed(&self, episode_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE revocations SET push_status='pushed', pushed_at=?2 WHERE episode_id=?1",
            params![episode_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn all_revoked_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT episode_id FROM revocations ORDER BY revoked_at ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.flatten().collect())
    }

    pub fn record_snapshot(
        &self,
        version: &str,
        train_count: usize,
        val_count: usize,
        manifest_hash: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots(version, built_at, train_count, val_count, manifest_hash, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)
             ON CONFLICT(version) DO UPDATE SET
               built_at=excluded.built_at,
               train_count=excluded.train_count,
               val_count=excluded.val_count,
               manifest_hash=excluded.manifest_hash",
            params![
                version,
                Utc::now().to_rfc3339(),
                train_count as i64,
                val_count as i64,
                manifest_hash
            ],
        )?;
        Ok(())
    }

    pub fn mark_snapshot_published(&self, version: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE snapshots SET published_at=?2 WHERE version=?1",
            params![version, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}
