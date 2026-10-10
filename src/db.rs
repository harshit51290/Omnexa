//! Local history in SQLite (`<backup root>\savevault.db`):
//! which games were seen when, backup records, and progress snapshots.
//! The filesystem stays the source of truth — the DB is an index.

use std::path::Path;

use crate::games::GameEntry;

fn db_path(backup_root: &Path) -> std::path::PathBuf {
    backup_root.join("savevault.db")
}

fn open(backup_root: &Path) -> Option<rusqlite::Connection> {
    std::fs::create_dir_all(backup_root).ok()?;
    let c = rusqlite::Connection::open(db_path(backup_root)).ok()?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS games(
            key TEXT PRIMARY KEY, name TEXT, root TEXT, source TEXT,
            first_seen TEXT, last_seen TEXT);
         CREATE TABLE IF NOT EXISTS backups(
            id INTEGER PRIMARY KEY, game_key TEXT, dir TEXT, created_at TEXT,
            kind TEXT, files INTEGER, bytes INTEGER);
         CREATE TABLE IF NOT EXISTS stats(
            game_key TEXT, scanned_at TEXT, saves INTEGER, bytes INTEGER,
            progress TEXT);",
    )
    .ok()?;
    Some(c)
}

fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Record one completed scan: upserts games + appends a progress snapshot.
pub fn record_scan(backup_root: &Path, games: &[GameEntry]) {
    let Some(c) = open(backup_root) else { return };
    let now = now_str();
    for g in games {
        let key = g.watch_key();
        let _ = c.execute(
            "INSERT INTO games(key,name,root,source,first_seen,last_seen)
             VALUES(?1,?2,?3,?4,?5,?5)
             ON CONFLICT(key) DO UPDATE SET name=excluded.name,last_seen=excluded.last_seen",
            rusqlite::params![key, g.name, g.root.to_string_lossy(), g.source, now],
        );
        let _ = c.execute(
            "INSERT INTO stats(game_key,scanned_at,saves,bytes,progress)
             VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![
                key,
                now,
                g.saves.len() as i64,
                g.total_bytes as i64,
                g.progress
            ],
        );
    }
    // Keep history lean: 60 snapshots per game.
    let _ = c.execute(
        "DELETE FROM stats WHERE rowid NOT IN
         (SELECT rowid FROM stats ORDER BY scanned_at DESC LIMIT 2000)",
        [],
    );
}

/// Record a finished backup (`manual`, `auto` or `snapshot`).
pub fn record_backup(
    backup_root: &Path,
    game_key: &str,
    dir: &Path,
    kind: &str,
    files: usize,
    bytes: u64,
) {
    let Some(c) = open(backup_root) else { return };
    let _ = c.execute(
        "INSERT INTO backups(game_key,dir,created_at,kind,files,bytes)
         VALUES(?1,?2,?3,?4,?5,?6)",
        rusqlite::params![
            game_key,
            dir.to_string_lossy(),
            now_str(),
            kind,
            files as i64,
            bytes as i64
        ],
    );
}

pub struct GameHistory {
    pub first_seen: Option<String>,
    pub backups: i64,
    pub history: Vec<(String, String)>,
}

/// First seen + backup count + recent progress snapshots for Details.
pub fn history(backup_root: &Path, game_key: &str) -> GameHistory {
    let mut h = GameHistory {
        first_seen: None,
        backups: 0,
        history: Vec::new(),
    };
    let Some(c) = open(backup_root) else { return h };
    if let Ok(v) = c.query_row(
        "SELECT first_seen FROM games WHERE key=?1",
        [game_key],
        |r| r.get::<_, String>(0),
    ) {
        h.first_seen = Some(v);
    }
    if let Ok(v) = c.query_row(
        "SELECT COUNT(*) FROM backups WHERE game_key=?1 AND kind!='snapshot'",
        [game_key],
        |r| r.get::<_, i64>(0),
    ) {
        h.backups = v;
    }
    if let Ok(mut stmt) = c.prepare(
        "SELECT scanned_at,progress FROM stats WHERE game_key=?1
         ORDER BY scanned_at DESC LIMIT 30",
    ) {
        if let Ok(rows) = stmt.query_map([game_key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            let mut last = String::new();
            for r in rows.flatten() {
                // Collapse repeats: history shows change, not noise.
                if r.1 != last {
                    last = r.1.clone();
                    h.history.push(r);
                }
                if h.history.len() >= 5 {
                    break;
                }
            }
        }
    }
    h
}
