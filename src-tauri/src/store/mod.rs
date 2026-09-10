//! Persistent project/asset/render history via bundled rusqlite (Phase 1).
//!
//! The webview never touches SQL directly; Rust owns all access here and the UI
//! observes results through `studio://event` (see `events.rs`). This module is
//! fully unit-tested against an in-memory connection.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod schema {
    //! Idempotent DDL. Uses `CREATE TABLE IF NOT EXISTS` so re-running migrations
    //! at a later version never errors on existing rows.
    pub const SCHEMA_SQL: &str = r#"
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            dir TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            harness TEXT NOT NULL DEFAULT 'a-coder-cli',
            model TEXT NOT NULL DEFAULT 'navya/auto',
            source TEXT NOT NULL DEFAULT 'cloud'
        );

        CREATE TABLE IF NOT EXISTS compositions (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            name TEXT NOT NULL,
            entry TEXT NOT NULL,       -- path to composition.html
            width INTEGER NOT NULL DEFAULT 1280,
            height INTEGER NOT NULL DEFAULT 720,
            fps INTEGER NOT NULL DEFAULT 30,
            duration REAL NOT NULL DEFAULT 0.0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS renders (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            composition_id TEXT,
            target TEXT NOT NULL DEFAULT 'local',   -- local|docker|cloud|lambda|cloudrun
            quality TEXT NOT NULL DEFAULT 'draft',   -- draft|high
            width INTEGER NOT NULL DEFAULT 1280,
            height INTEGER NOT NULL DEFAULT 720,
            fps INTEGER NOT NULL DEFAULT 30,
            status TEXT NOT NULL DEFAULT 'queued',   -- queued|running|done|failed|cancelled
            started_at INTEGER,
            finished_at INTEGER,
            output_path TEXT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS assets (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            composition_id TEXT,          -- nullable: free-floating / unassigned media
            path TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'image',  -- image|video|audio
            source TEXT NOT NULL DEFAULT 'cloud',   -- cloud|local (generation route)
            prompt TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS generation_log (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'image',  -- image|chat|video|audio
            model TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'cloud',
            prompt TEXT,
            cost_usd REAL NOT NULL DEFAULT 0.0,
            tokens INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_compositions_project ON compositions(project_id);
        CREATE INDEX IF NOT EXISTS idx_renders_project ON renders(project_id);
        CREATE INDEX IF NOT EXISTS idx_assets_project ON assets(project_id);
    "#;

    /// Current schema version (bump on every breaking change).
    /// Schema version — incremented when DDL changes require migration.
    #[allow(dead_code)]
    pub const VERSION: i64 = 1;
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// A tiny typed accessor over the project table. Kept minimal for Phase 1; the
/// render/asset/project helpers grow in later phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub created_at_ms: i64,
    pub harness: String,
    pub model: String,
    pub source: String,
}

/// A live connection with migrations applied. Created by the app at startup; in
/// tests we construct an in-memory connection to exercise the schema.
///
/// `rusqlite::Connection` is `Send` but not `Sync`; we declare the store `Sync`
/// so it can live behind a `parking_lot::Mutex` in `AppState` while keeping
/// async command futures `Send` (the sync guard is dropped before any `.await`).
pub struct ProjectStore {
    pub(crate) conn: rusqlite::Connection,
}

// SAFETY: access is serialized entirely through the enclosing parking_lot mutex.
unsafe impl Sync for ProjectStore {}

impl ProjectStore {
    /// Run all `CREATE TABLE IF NOT EXISTS` statements (idempotent). Safe to call
    /// repeatedly — no rows are ever deleted or duplicated by a re-run.
    pub fn migrate(conn: &rusqlite::Connection) -> Result<(), StoreError> {
        conn.execute_batch(schema::SCHEMA_SQL)?;
        Ok(())
    }

    /// Connection tuning (open-design lesson B3): WAL so readers never block
    /// the writer (the UI polls while renders write), `foreign_keys` on so
    /// orphan rows are structurally impossible, NORMAL sync (safe under WAL),
    /// and a busy timeout so a checkpoint/sidecar write never errors out.
    fn configure(conn: &rusqlite::Connection) -> Result<(), StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(StoreError::Sqlite)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(StoreError::Sqlite)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(StoreError::Sqlite)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open(path)?;
        Self::configure(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Open a throwaway in-memory store and run migrations + the same tuning
    /// as production (foreign_keys ON etc.), so tests see FK behavior.
    #[cfg(test)]
    pub fn memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory().map_err(StoreError::Sqlite)?;
        Self::configure(&conn)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Insert a project row. Returns it for convenience.
    pub fn create_project(
        &self,
        id: &str,
        name: &str,
        dir: &str,
        harness: &str,
        model: &str,
        source: &str,
    ) -> Result<ProjectRow, StoreError> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO projects (id, name, dir, created_at, harness, model, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, name, dir, now, harness, model, source],
        )?;
        Ok(ProjectRow {
            id: id.to_string(),
            name: name.to_string(),
            dir: dir.to_string(),
            created_at_ms: now,
            harness: harness.to_string(),
            model: model.to_string(),
            source: source.to_string(),
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRow>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT id, name, dir, created_at, harness, model, source FROM projects ORDER BY created_at")?;
        let rows = stmt.query_map([], |r| {
            Ok(ProjectRow {
                id: r.get(0)?,
                name: r.get(1)?,
                dir: r.get(2)?,
                created_at_ms: r.get(3)?,
                harness: r.get(4)?,
                model: r.get(5)?,
                source: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Delete a project row and all its dependent rows (assets, compositions,
    /// renders, generation log) in one transaction. Files on disk are left
    /// alone (deletion is metadata-level, never destructive to media).
    /// Explicit deletes rather than FK cascade: older DBs may predate the
    /// foreign_keys pragma, and asset rows carry no FK at all.
    /// Returns true when a row was actually removed.
    pub fn delete_project(&self, project_id: &str) -> Result<bool, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        for table in ["assets", "compositions", "renders", "generation_log"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE project_id = ?1"),
                rusqlite::params![project_id],
            )?;
        }
        let n = tx.execute(
            "DELETE FROM projects WHERE id = ?1",
            rusqlite::params![project_id],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Repair project rows whose `dir` can't possibly be a real project tree:
    /// relative paths (the onboarding "blank project" used to store a literal
    /// "."), empty strings, or directories that don't exist. `resolve` maps a
    /// broken row to its corrected directory (kept as a callback so tests can
    /// drive it without an app handle).
    ///
    /// - relative/placeholder dirs → relocated to `resolve(row)` (rewritten)
    /// - absolute-but-missing dirs → re-created in place (the user's recorded
    ///   location is kept; relocating would orphan their files)
    /// Returns the rows whose dir was rewritten so callers can log/emit events.
    pub fn repair_project_dirs(
        &self,
        resolve: impl Fn(&ProjectRow) -> PathBuf,
    ) -> Result<Vec<ProjectRow>, StoreError> {
        let projects = self.list_projects()?;
        let mut repaired = Vec::new();
        for mut p in projects {
            let path = std::path::Path::new(&p.dir);
            if p.dir.trim().is_empty() || p.dir == "." || !path.is_absolute() {
                let dir = resolve(&p);
                if std::fs::create_dir_all(&dir).is_ok() {
                    p.dir = dir.to_string_lossy().to_string();
                    self.conn.execute(
                        "UPDATE projects SET dir = ?1 WHERE id = ?2",
                        rusqlite::params![p.dir, p.id],
                    )?;
                    repaired.push(p);
                }
            } else if !path.is_dir() {
                let _ = std::fs::create_dir_all(path);
            }
        }
        Ok(repaired)
    }

    /// Insert an asset row (idempotent on the derived id). Returns the asset id.
    pub fn insert_asset(
        &self,
        project_id: &str,
        composition_id: Option<&str>,
        path: &str,
        kind: &str,
        source: &str,
        prompt: Option<&str>,
    ) -> Result<String, StoreError> {
        let id = format!("a-{}", hash_str(path));
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO assets (id, project_id, composition_id, path, kind, source, prompt, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![id, project_id, composition_id, path, kind, source, prompt, now],
        )?;
        Ok(id)
    }

    /// Count rows in a table (used by the generation-log / asset row counts).
    pub fn count(&self, table: &str) -> Result<i64, StoreError> {
        // Validate table name to prevent SQL injection (only allow known tables).
        let allowed = ["projects", "compositions", "renders", "assets", "generation_log"];
        if !allowed.contains(&table) {
            return Err(StoreError::Sqlite(rusqlite::Error::InvalidQuery));
        }
        let sql = format!("SELECT COUNT(*) FROM {}", table);
        let n = self.conn.query_row(&sql, [], |r| r.get::<_, i64>(0))?;
        Ok(n)
    }

    // --- Render-queue persistence (Phase 14 hardening) ---------------------

    /// Upsert a render job row (source of truth for the queue across
    /// restarts). Called by the render pipeline on every queue mutation.
    pub fn upsert_render(&self, job: &crate::render::RenderJob) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO renders
             (id, project_id, composition_id, target, quality, width, height, fps,
              status, started_at, finished_at, output_path, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                job.job_id,
                job.project_id,
                job.composition_id,
                format!("{:?}", job.target).to_lowercase(),
                format!("{:?}", job.quality).to_lowercase(),
                job.width,
                job.height,
                job.fps,
                format!("{:?}", job.status).to_lowercase(),
                job.started_at_ms,
                job.finished_at_ms,
                job.output_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                job.error,
            ],
        )?;
        Ok(())
    }

    /// All render rows, oldest-start first (queue order). The SQLite table is
    /// the durable source of truth for the render queue.
    pub fn list_renders(&self) -> Result<Vec<crate::render::RenderJob>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, composition_id, target, quality, width, height, fps,
                    status, started_at, finished_at, output_path, error
             FROM renders ORDER BY COALESCE(started_at, 0) ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(RenderRow {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    composition_id: r.get(2)?,
                    target: r.get(3)?,
                    quality: r.get(4)?,
                    width: r.get(5)?,
                    height: r.get(6)?,
                    fps: r.get(7)?,
                    status: r.get(8)?,
                    started_at: r.get(9)?,
                    finished_at: r.get(10)?,
                    output_path: r.get(11)?,
                    error: r.get(12)?,
                })
            })?
            .filter_map(Result::ok)
            .map(|row| row.into_job())
            .collect();
        Ok(rows)
    }

    /// Mark renders still "running" (from a crashed previous session) as
    /// failed, returning the reconciled jobs so callers can persist them.
    pub fn reconcile_stale_renders(&self) -> Result<Vec<crate::render::RenderJob>, StoreError> {
        let mut jobs = self.list_renders()?;
        let mut changed = Vec::new();
        for job in jobs.iter_mut() {
            if job.status == crate::render::RenderStatus::Running {
                job.status = crate::render::RenderStatus::Failed;
                job.error = Some("app restarted while this render was running".to_string());
                job.finished_at_ms = Some(now_ms());
                self.upsert_render(job)?;
                changed.push(job.clone());
            }
        }
        Ok(changed)
    }
}

/// Raw renders-table row, mapped to/from `render::RenderJob`.
struct RenderRow {
    id: String,
    project_id: String,
    composition_id: String,
    target: String,
    quality: String,
    width: i64,
    height: i64,
    fps: i64,
    status: String,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    output_path: Option<String>,
    error: Option<String>,
}

impl RenderRow {
    fn into_job(self) -> crate::render::RenderJob {
        use crate::render::{RenderJob, RenderQuality, RenderStatus, RenderTarget};
        let target = match self.target.as_str() {
            "docker" => RenderTarget::Docker,
            "cloud" => RenderTarget::Cloud,
            "lambda" => RenderTarget::Lambda,
            "cloudrun" => RenderTarget::CloudRun,
            _ => RenderTarget::Local,
        };
        let quality = match self.quality.as_str() {
            "high" => RenderQuality::High,
            _ => RenderQuality::Draft,
        };
        let status = match self.status.as_str() {
            "running" => RenderStatus::Running,
            "done" => RenderStatus::Done,
            "failed" => RenderStatus::Failed,
            "cancelled" => RenderStatus::Cancelled,
            _ => RenderStatus::Queued,
        };
        RenderJob {
            job_id: self.id,
            project_id: self.project_id,
            composition_id: self.composition_id,
            target,
            quality,
            width: self.width.max(1) as u32,
            height: self.height.max(1) as u32,
            fps: self.fps.max(1) as u32,
            status,
            started_at_ms: self.started_at,
            finished_at_ms: self.finished_at,
            output_path: self.output_path.map(PathBuf::from),
            error: self.error,
        }
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Deterministic 8-hex-char hash for derived ids (asset rows, etc.).
fn hash_str(s: &str) -> String {
    let mut h: u32 = 0;
    for c in s.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    format!("{:08x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_persistence_roundtrip_and_reconcile() {
        use crate::render::{RenderJob, RenderStatus, RenderTarget};
        let store = ProjectStore::memory().unwrap();

        let job = RenderJob {
            job_id: "j1".into(),
            project_id: "p1".into(),
            composition_id: "c1".into(),
            target: RenderTarget::Cloud,
            status: RenderStatus::Running,
            started_at_ms: Some(1000),
            ..RenderJob::default()
        };
        store.upsert_render(&job).unwrap();

        let rows = store.list_renders().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target, RenderTarget::Cloud);
        assert_eq!(rows[0].status, RenderStatus::Running);
        assert_eq!(rows[0].started_at_ms, Some(1000));

        // A running job from a crashed previous session reconciles to failed.
        let changed = store.reconcile_stale_renders().unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].status, RenderStatus::Failed);
        assert!(changed[0].error.clone().unwrap_or_default().contains("restarted"));
        // Second reconcile is a no-op.
        assert!(store.reconcile_stale_renders().unwrap().is_empty());

        // Terminal rows survive a re-upsert with new fields.
        let mut done = changed[0].clone();
        done.status = RenderStatus::Done;
        done.output_path = Some(std::path::PathBuf::from("renders/out.mp4"));
        store.upsert_render(&done).unwrap();
        let rows = store.list_renders().unwrap();
        assert_eq!(rows[0].status, RenderStatus::Done);
        assert_eq!(rows[0].output_path.as_deref(), Some(std::path::Path::new("renders/out.mp4")));
    }

    #[test]
    fn pragmas_are_set_for_runtime_concurrency() {
        // WAL + foreign_keys + NORMAL sync — set by ProjectStore::new (lesson
        // B3 from open-design). In-memory connections report journal_mode
        // "memory", so verify via a real temp file DB.
        let path = std::env::temp_dir().join(format!("navya-store-pragma-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = ProjectStore::new(&path).unwrap();
        let journal: String = store
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        let fk: i64 = store.conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn schema_columns_present_per_pragma_table_info() {
        // PRAGMA-driven column check (open-design db.ts pattern): assert the
        // columns newer migrations add are actually there, independent of the
        // DDL text.
        let store = ProjectStore::memory().unwrap();
        let mut stmt = store.conn.prepare("PRAGMA table_info(projects)").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for expected in ["id", "name", "dir", "created_at", "harness", "model", "source"] {
            assert!(cols.contains(&expected.to_string()), "projects missing {expected}: {cols:?}");
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let store = ProjectStore::memory().unwrap();
        assert!(ProjectStore::migrate(&store.conn).is_ok());
        // Running again must not error or duplicate the tables.
        assert!(ProjectStore::migrate(&store.conn).is_ok());
        for t in ["projects", "compositions", "renders", "assets", "generation_log"] {
            assert_eq!(store.count(t).unwrap(), 0);
        }
    }

    #[test]
    fn delete_project_removes_row_and_cascades() {
        let store = ProjectStore::memory().unwrap();
        store.create_project("p1", "doomed", "/x", "a-coder-cli", "m", "cloud").unwrap();
        store.insert_asset("p1", None, "/x/a.png", "image", "local", None).unwrap();
        assert_eq!(store.delete_project("missing").unwrap(), false);
        assert_eq!(store.delete_project("p1").unwrap(), true);
        assert!(store.list_projects().unwrap().is_empty());
        // FK cascade removed the orphaned asset row.
        assert_eq!(store.count("assets").unwrap(), 0);
    }

    #[test]
    fn repair_project_dirs_fixes_placeholder_and_relative_dirs() {
        let base = std::env::temp_dir().join(format!("navya-repair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let store = ProjectStore::memory().unwrap();
        // The historical onboarding bug: literal "." stored as the project dir.
        store.create_project("p1", "My first video", ".", "a-coder-cli", "m", "cloud").unwrap();
        // Relative path variant.
        store.create_project("p2", "relative", "some/relative/path", "a-coder-cli", "m", "cloud").unwrap();
        // A valid absolute project must be left untouched.
        let good_dir = base.join("good");
        std::fs::create_dir_all(&good_dir).unwrap();
        store.create_project("p3", "good", good_dir.to_str().unwrap(), "a-coder-cli", "m", "cloud").unwrap();

        let repaired = store
            .repair_project_dirs(|row| base.join("projects").join(&row.id))
            .unwrap();
        assert_eq!(repaired.len(), 2, "p1+p2 repaired, p3 untouched");
        let after = store.list_projects().unwrap();
        let p1 = after.iter().find(|p| p.id == "p1").unwrap();
        assert_eq!(PathBuf::from(&p1.dir), base.join("projects").join("p1"));
        assert!(PathBuf::from(&p1.dir).is_dir());
        let p3 = after.iter().find(|p| p.id == "p3").unwrap();
        assert_eq!(p3.dir, good_dir.to_string_lossy());
        // Second run is a no-op (dirs now absolute + existing).
        assert!(store.repair_project_dirs(|row| base.join("projects").join(&row.id)).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn repair_creates_missing_absolute_dir_instead_of_moving() {
        let base = std::env::temp_dir().join(format!("navya-repair2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let store = ProjectStore::memory().unwrap();
        let moved = base.join("moved-away");
        store.create_project("p1", "gone", moved.to_str().unwrap(), "a-coder-cli", "m", "cloud").unwrap();
        let repaired = store
            .repair_project_dirs(|row| base.join("projects").join(&row.id))
            .unwrap();
        // The absolute path was missing — it is RE-CREATED in place, not
        // relocated, so files the user may restore land back in place.
        assert!(repaired.is_empty());
        assert!(moved.is_dir());
        assert_eq!(store.list_projects().unwrap()[0].dir, moved.to_string_lossy());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn project_round_trip() {
        let store = ProjectStore::memory().unwrap();
        let row = store.create_project("p1", "black-holes-explainer", "/proj/abc", "a-coder-cli", "navya/auto", "cloud").unwrap();
        // The row was created at "now"; the clock can tick forward between the
        // insert and this assertion, so assert monotonicity, not strict ==.
        assert!(row.created_at_ms <= now_ms(), "created_at_ms should be now or earlier");

        let list = store.list_projects().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p1");
        assert_eq!(list[0].source, "cloud");
    }

    #[test]
    fn primary_key_conflict_fails() {
        let store = ProjectStore::memory().unwrap();
        store.create_project("dup", "a", "/x", "a-coder-cli", "m", "cloud").unwrap();
        let err = store.create_project("dup", "b", "/y", "a-coder-cli", "m", "cloud").unwrap_err();
        assert!(matches!(err, StoreError::Sqlite(_)));
    }
}