//! Backup + restore for any detected game. Opaque files are copied verbatim;
//! FromSoftware `.sl2` files get their fingerprints re-signed on restore.

use crate::games::{scan_files, GameEntry};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn default_backup_root() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\A3\Documents"))
        .join("SaveVault")
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn sanitize(s: &str) -> String {
    // Keep it readable: alphanumerics, dash, dot; rest become underscore.
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c.to_string()
            } else if c == ' ' {
                "-".to_string()
            } else {
                "_".to_string()
            }
        })
        .collect();
    out.truncate(60);
    if out.is_empty() {
        out = "game".into();
    }
    out
}

pub fn dir_name(entry: &GameEntry) -> String {
    sanitize(&entry.name.to_ascii_lowercase().replace(' ', "-"))
}

/// Copy every detected save file into a timestamped folder with a manifest.
pub fn backup_game(entry: &GameEntry, root: &Path, keep: usize) -> Result<PathBuf> {
    if entry.saves.is_empty() {
        anyhow::bail!("nothing to back up yet");
    }
    let dest = root.join(dir_name(entry)).join(stamp());
    std::fs::create_dir_all(dest.join("saves"))?;
    let mut files = Vec::new();
    for f in &entry.saves {
        let flat = sanitize(&f.path.to_string_lossy());
        let target = dest.join("saves").join(format!("{flat}.bin"));
        std::fs::copy(&f.path, &target).map_err(|e| friendly_io(&e, &f.path))?;
        files.push(serde_json::json!({
            "src": f.path.to_string_lossy(),
            "stored_as": target.file_name().unwrap_or_default().to_string_lossy(),
            "size": f.size,
        }));
    }
    let manifest = serde_json::json!({
        "kind": "unified",
        "name": entry.name,
        "root": entry.root.to_string_lossy(),
        "exe": entry.exe.as_ref().map(|p| p.to_string_lossy().to_string()),
        "source": entry.source,
        "backed_up_at": chrono::Local::now().to_rfc3339(),
        "files": files,
    });
    std::fs::write(
        dest.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    prune(root, &dir_name(entry), keep.max(1));
    Ok(dest)
}

/// Snapshot current saves before a restore overwrites them.
pub fn snapshot(entry: &GameEntry, root: &Path) -> Result<PathBuf> {
    let dest = root
        .join(dir_name(entry))
        .join(format!("pre-restore-{}", stamp()));
    std::fs::create_dir_all(dest.join("saves"))?;
    let mut files = Vec::new();
    for f in &entry.saves {
        if !f.path.is_file() {
            continue;
        }
        let flat = sanitize(&f.path.to_string_lossy());
        let target = dest.join("saves").join(format!("{flat}.bin"));
        if std::fs::copy(&f.path, &target).is_ok() {
            files.push(serde_json::json!({
                "src": f.path.to_string_lossy(),
                "stored_as": target.file_name().unwrap_or_default().to_string_lossy(),
            }));
        }
    }
    let manifest = serde_json::json!({
        "kind": "snapshot",
        "name": entry.name,
        "backed_up_at": chrono::Local::now().to_rfc3339(),
        "files": files,
    });
    std::fs::write(
        dest.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(dest)
}

/// Put a backup's files back where the manifest says they came from.
pub fn restore_backup(backup_dir: &Path) -> Result<(usize, u64)> {
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(backup_dir.join("manifest.json"))?)?;
    let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let mut n = 0usize;
    let mut bytes = 0u64;
    // Flat manifests (auto/unified/snapshot): stored under saves/.
    // Legacy curated manifests: stored_as is relative to the backup dir.
    let flat = matches!(kind, "auto" | "unified" | "snapshot");
    for f in m
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let src = f.get("src").and_then(|v| v.as_str()).unwrap_or("");
        let stored = f.get("stored_as").and_then(|v| v.as_str()).unwrap_or("");
        if src.is_empty() || stored.is_empty() {
            continue;
        }
        let from = if flat {
            backup_dir.join("saves").join(stored)
        } else {
            backup_dir.join(stored)
        };
        let to = PathBuf::from(src);
        if !from.is_file() {
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| friendly_io(&e, parent))?;
        }
        let b = std::fs::copy(&from, &to).map_err(|e| friendly_io(&e, &to))?;
        if to
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("sl2"))
            == Some(true)
        {
            let _ = crate::bnd4::resign_sl2(&to);
        }
        n += 1;
        bytes += b;
    }
    if n == 0 {
        anyhow::bail!("backup is empty — nothing was restored");
    }
    Ok((n, bytes))
}

/// Keep only the newest `keep` backups for a game.
fn prune(root: &Path, dir: &str, keep: usize) {
    let mut v: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join(dir)) {
        for e in rd
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("manifest.json").exists())
        {
            v.push(e.path());
        }
    }
    v.sort();
    v.reverse();
    for old in v.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(old);
    }
}

pub fn list_backups_for(root: &Path, dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join(dir)) {
        for e in rd
            .flatten()
            .filter(|e| e.path().is_dir() && e.path().join("manifest.json").exists())
        {
            v.push(e.path());
        }
    }
    v.sort();
    v.reverse();
    v.truncate(10);
    v
}

/// Total size of everything under the backup root (bounded walk).
pub fn backup_total(root: &Path) -> u64 {
    fn walk(d: &Path, acc: &mut u64, n: &mut usize) {
        if *n > 5000 {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if *n > 5000 {
                    return;
                }
                let p = e.path();
                if p.is_file() {
                    *n += 1;
                    *acc += e.metadata().map(|m| m.len()).unwrap_or(0);
                } else if p.is_dir() {
                    walk(&p, acc, n);
                }
            }
        }
    }
    let mut acc = 0;
    let mut n = 0;
    walk(root, &mut acc, &mut n);
    acc
}

/// FromSoftware twin check: game keeps `.sl2` + identical `.sl2.bak`.
/// Returns a warning when they differ (unclean shutdown), else None.
pub fn twin_status(entry: &GameEntry) -> Option<String> {
    let mut checked = false;
    for f in &entry.saves {
        let s = f.path.to_string_lossy().to_string();
        if s.ends_with(".sl2") && !s.ends_with(".sl2.bak") {
            let bak = PathBuf::from(format!("{s}.bak"));
            if bak.is_file() {
                checked = true;
                let a = std::fs::metadata(&f.path).ok()?.len();
                let b = std::fs::metadata(&bak).ok()?.len();
                if a != b {
                    return Some(
                        "Heads up: the game didn't shut down cleanly last time. Your backup is still fine.".into(),
                    );
                }
            }
        }
    }
    if checked {
        None
    } else {
        None
    }
}

/// Re-scan an entry's save folders (cheap refresh after backup/restore).
pub fn refresh_saves(entry: &mut GameEntry) {
    let mut saves = Vec::new();
    for sd in &entry.save_dirs {
        if sd.is_dir() {
            saves.extend(scan_files(sd, 60));
        }
    }
    // Include in-install extras again.
    for extra in [
        "steam_settings",
        "savegames",
        "saves",
        "SaveGames",
        "SAVEDGAMES",
        "Profiles",
    ] {
        let p = entry.root.join(extra);
        if p.is_dir() {
            for f in scan_files(&p, 30) {
                if !saves.iter().any(|x| x.path == f.path) {
                    saves.push(f);
                }
            }
        }
    }
    saves.sort_by_key(|f| f.path.clone());
    saves.truncate(60);
    entry.saves = saves;
    entry.total_bytes = entry.saves.iter().map(|f| f.size).sum();
    entry.last_played = entry.saves.iter().filter_map(|f| f.mtime).max();
}

/// Turn OS errors into sentences a non-technical user can act on.
pub fn friendly_err(e: &anyhow::Error) -> String {
    friendly_text(&e.to_string())
}

fn friendly_text(s: &str) -> String {
    let low = s.to_ascii_lowercase();
    if low.contains("access is denied") || low.contains("os error 5") {
        "Windows blocked access — try running SaveVault as administrator.".into()
    } else if low.contains("os error 112") || low.contains("no space") || low.contains("disk full")
    {
        "Disk is full — free some space and try again.".into()
    } else if low.contains("not found") || low.contains("os error 3") || low.contains("os error 2")
    {
        "A file moved or was deleted — hit Re-scan and try again.".into()
    } else if low.contains("being used by another process") || low.contains("os error 32") {
        "The game (or another program) is using that file — quit the game first.".into()
    } else {
        format!("Didn't work ({s}). Nothing was changed.")
    }
}

fn friendly_io(e: &std::io::Error, _p: &Path) -> anyhow::Error {
    anyhow::anyhow!(friendly_text(&e.to_string()))
}
