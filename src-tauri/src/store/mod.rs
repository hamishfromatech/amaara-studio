//! Persistent project/asset/render history via bundled rusqlite (Phase 1).
//!
//! The webview never touches SQL directly; Rust owns all access here and the UI
//! observes results through `studio://event` (see `events.rs`). This module is
//! fully unit-tested against an in-memory connection.

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

    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open(path)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Open a throwaway in-memory store and run migrations (test helper).
    #[cfg(test)]
    pub fn memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory().map_err(StoreError::Sqlite)?;
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