use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

/// Shared application state handed to every Tauri command.
///
/// The SQLite connection is guarded by a std Mutex; commands run blocking
/// work inside `spawn_blocking`, so no async-aware lock is required.
pub struct AppState {
    pub conn: Mutex<Connection>,
    pub data_dir: PathBuf,
}

impl AppState {
    /// Create app dirs, open the database and run migrations + FTS setup.
    pub fn init(data_dir: PathBuf) -> Result<Self, String> {
        crate::storage::files::ensure_dirs(&data_dir)?;
        let db_path = data_dir.join("app.db");
        let mut conn = crate::db::open_db(&db_path)?;
        crate::db::migrations::run(&mut conn)?;
        crate::db::migrations::ensure_fts(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            data_dir,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("app.db")
    }

    pub fn files_root(&self) -> PathBuf {
        crate::storage::files::files_dir(&self.data_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    #[test]
    fn init_migrations_fts_and_cascade() {
        let dir = std::env::temp_dir().join(format!("cmt-test-{}", uuid::Uuid::new_v4()));
        let st = AppState::init(dir.clone()).expect("state init must succeed");
        let conn = st.conn.lock().unwrap();

        // Bundled SQLite ships FTS5; the app must have wired it up.
        assert!(crate::db::fts_enabled(&conn));

        let now = crate::db::now_iso();
        conn.execute(
            "INSERT INTO roots(id,name,color,icon,created_at,updated_at)
             VALUES('r1','School',NULL,NULL,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n1','r1',NULL,'folder','Physics',0,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes(id,root_id,parent_id,type,name,position,created_at,updated_at)
             VALUES('n2','r1','n1','chat','Gravity Chat',0,?1,?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chats(id,node_id,title,source,raw_path,first_idea,tags,brief_text,created_at,updated_at)
             VALUES('c1','n2','Gravity Chat','manual','/tmp/x','why apples fall','physics',
                    'brief about gravity and mass',?1,?1)",
            [&now],
        )
        .unwrap();

        // FTS finds freshly inserted content.
        let hit: Option<String> = conn
            .query_row(
                "SELECT ch.title FROM chat_fts f JOIN chats ch ON ch.rowid = f.rowid
                 WHERE chat_fts MATCH ?1",
                ["gravity"],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(hit.as_deref(), Some("Gravity Chat"));

        // Updates propagate through the sync trigger.
        conn.execute(
            "UPDATE chats SET brief_text = 'now mentions quantum tunneling' WHERE id = 'c1'",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM chat_fts WHERE chat_fts MATCH 'quantum'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);

        // Deleting the root cascades chats AND removes FTS rows.
        conn.execute("DELETE FROM roots WHERE id = 'r1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM chat_fts WHERE chat_fts MATCH 'gravity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);

        drop(conn);
        let _ = std::fs::remove_dir_all(dir);
    }
}
