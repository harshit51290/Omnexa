use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub backup_root: PathBuf,
    pub auto_backup: bool,
    /// Watch keys: "<install root>|<exe file name>".
    pub watched: Vec<String>,
    #[serde(default = "default_poll")]
    pub poll_secs: u64,
    #[serde(default = "default_retention")]
    pub retention: usize,
    #[serde(default)]
    pub custom_folders: Vec<PathBuf>,
    /// SteamGridDB API key (free at steamgriddb.com) for cover art.
    #[serde(default)]
    pub sgdb_key: String,
    /// Remembered window size (default 1440x900).
    #[serde(default)]
    pub win_w: f32,
    #[serde(default)]
    pub win_h: f32,
}

fn default_poll() -> u64 {
    10
}

fn default_retention() -> usize {
    8
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backup_root: crate::core::backup::default_backup_root(),
            auto_backup: false,
            watched: Vec::new(),
            poll_secs: 10,
            retention: 8,
            custom_folders: Vec::new(),
            sgdb_key: String::new(),
            win_w: 1440.0,
            win_h: 900.0,
        }
    }
}

pub fn load(backup_root: &std::path::Path) -> Settings {
    let p = backup_root.join("settings.json");
    if let Ok(t) = std::fs::read_to_string(&p) {
        if let Ok(mut s) = serde_json::from_str::<Settings>(&t) {
            s.backup_root = backup_root.to_path_buf();
            if s.retention == 0 || s.retention > 100 {
                s.retention = 8;
            }
            if s.win_w < 800.0 {
                s.win_w = 1440.0;
            }
            if s.win_h < 500.0 {
                s.win_h = 900.0;
            }
            // Drop legacy keys from the hardcoded-list era.
            s.watched.retain(|w| !w.starts_with("curated:"));
            // Strip the previous "auto:" prefix so old keys keep matching.
            s.watched = s
                .watched
                .into_iter()
                .map(|w| w.strip_prefix("auto:").unwrap_or(&w).to_string())
                .collect();
            return s;
        }
    }
    Settings {
        backup_root: backup_root.to_path_buf(),
        ..Default::default()
    }
}

pub fn save(s: &Settings) {
    let _ = std::fs::create_dir_all(&s.backup_root);
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(s.backup_root.join("settings.json"), t);
    }
    remember_root(&s.backup_root);
}

/// Canonical home (new installs). The actual root may be a custom folder.
pub fn home_root() -> PathBuf {
    crate::core::backup::default_backup_root()
}

fn active_pointer() -> PathBuf {
    home_root().join("active_root.txt")
}

/// Persist which folder is live so a custom choice survives restarts.
pub fn remember_root(p: &std::path::Path) {
    if !p.is_absolute() {
        return;
    }
    let _ = std::fs::create_dir_all(home_root());
    let _ = std::fs::write(active_pointer(), p.to_string_lossy().as_bytes());
}

/// Boot root: remembered custom folder when it still exists, else default.
pub fn resolve_root() -> PathBuf {
    let d = crate::core::backup::default_backup_root();
    if let Ok(t) = std::fs::read_to_string(d.join("active_root.txt")) {
        let p = PathBuf::from(t.trim());
        if p.is_absolute() && p.is_dir() {
            return p;
        }
    }
    d
}

/// Lowercase exe names currently watched.
pub fn watched_exes(backup_root: &std::path::Path) -> HashSet<String> {
    let s = load(backup_root);
    if !s.auto_backup {
        return HashSet::new();
    }
    let mut out = HashSet::new();
    for w in &s.watched {
        let (root, exe) = match w.split_once('|') {
            Some((r, e)) => (r, e),
            None => (w.as_str(), ""),
        };
        if !exe.is_empty() {
            out.insert(exe.to_ascii_lowercase());
        } else {
            // Legacy key without exe: resolve install exes now.
            let p = PathBuf::from(root);
            if let Some(found) = crate::core::games::find_exe(&p) {
                if let Some(n) = found.file_name().and_then(|x| x.to_str()) {
                    out.insert(n.to_ascii_lowercase());
                }
            }
        }
    }
    out
}

pub fn running_exes() -> HashSet<String> {
    let mut out = HashSet::new();
    let r = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output();
    if let Ok(o) = r {
        let txt = String::from_utf8_lossy(&o.stdout);
        for line in txt.lines() {
            let line = line.trim();
            if line.len() < 6 || !line.starts_with('"') {
                continue;
            }
            if let Some(end) = line[1..].find('"') {
                out.insert(line[1..1 + end].to_ascii_lowercase());
            }
        }
    }
    out
}

/// True when any of these exe names is running right now.
#[allow(dead_code)]
pub fn is_running(names: &[String]) -> bool {
    if names.is_empty() {
        return false;
    }
    let running = running_exes();
    names
        .iter()
        .any(|n| running.contains(&n.to_ascii_lowercase()))
}

/// Background thread: when a watched game process disappears (was running,
/// now gone), report its exe name. Polls the settings file so the UI stays
/// in charge with zero IPC.
pub fn spawn_watchdog(backup_root: PathBuf, tx: Sender<String>) {
    std::thread::spawn(move || {
        let mut was_running: HashSet<String> = HashSet::new();
        loop {
            let watched = watched_exes(&backup_root);
            if !watched.is_empty() {
                let now = running_exes();
                let now_watched: HashSet<String> = watched.intersection(&now).cloned().collect();
                for exe in was_running.difference(&now_watched) {
                    let _ = tx.send(exe.clone());
                }
                was_running = now_watched;
            } else {
                was_running.clear();
            }
            let secs = load(&backup_root).poll_secs.clamp(5, 300);
            std::thread::sleep(Duration::from_secs(secs));
        }
    });
}
