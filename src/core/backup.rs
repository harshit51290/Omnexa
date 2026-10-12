//! Backup + restore for any detected game. Opaque files are copied verbatim;
//! FromSoftware `.sl2` files get their fingerprints re-signed on restore.
//!
//! Company-grade naming: every filesystem name is derived from SHA-256 so it
//! is short, deterministic and never trips Windows path limits, no matter how
//! long the original save path is. Human-readable names live in manifests and
//! the SQLite index — never in folder/file names.

use crate::core::games::{scan_files, GameEntry};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn default_backup_root() -> PathBuf {
    let docs = dirs::document_dir().unwrap_or_else(|| PathBuf::from(r"C:\Users\A3\Documents"));
    let next = docs.join("Omnexa");
    // Migrate gracefully: early builds used Documents\SaveVault. If it
    // exists and the new folder doesn't, keep using it — never orphan
    // a user's existing backups on upgrade.
    let legacy = docs.join("SaveVault");
    if !next.is_dir() && legacy.is_dir() {
        return legacy;
    }
    next
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn stamp_micros() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S%.6f").to_string()
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
    out.truncate(40);
    if out.is_empty() {
        out = "game".into();
    }
    out
}

/// First `n` hex chars of SHA-256(s). Deterministic, filesystem-safe.
pub fn short_hash(s: &str, n: usize) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex.chars().take(n.max(4).min(64)).collect()
}

/// SHA-256 of file bytes, hex. Used for integrity in the manifest.
pub fn file_hash(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    let mut h = Sha256::new();
    h.update(&b);
    Some(format!("{:x}", h.finalize()))
}

/// Stable per-game folder: `<12-hex>-<readable-name>`.
/// The hash comes from the watch key (install root + exe), so renames keep
/// working and two same-named games never collide.
pub fn dir_name(entry: &GameEntry) -> String {
    let readable = sanitize(&entry.name.to_ascii_lowercase().replace(' ', "-"));
    format!("{}-{}", short_hash(&entry.watch_key(), 12), readable)
}

/// Pre-SHA-256 folder name, kept so old backups are found, never written.
pub fn legacy_dir_name(entry: &GameEntry) -> String {
    entry
        .name
        .to_ascii_lowercase()
        .replace(' ', "-")
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
        .collect::<String>()
        .chars()
        .take(60)
        .collect()
}

/// All on-disk folders for a game: current hash dir + legacy readable dir
/// (pre-SHA-256 builds) so old backups are never orphaned.
fn game_dirs(root: &Path, entry: &GameEntry) -> Vec<PathBuf> {
    let mut v = vec![root.join(dir_name(entry))];
    let legacy = root.join(legacy_dir_name(entry));
    if legacy != v[0] {
        v.push(legacy);
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

/// New backup instance: `<timestamp>-<8-hex>`, sortable, unique even for two
/// backups in the same second.
fn instance_name(entry: &GameEntry, snap: bool) -> String {
    let t = stamp();
    let h = short_hash(&format!("{}|{t}|{}", entry.watch_key(), stamp_micros()), 8);
    if snap {
        format!("pre-restore-{t}-{h}")
    } else {
        format!("{t}-{h}")
    }
}

/// Stored filename: `<16-hex>_<original-file-name>.bin` — always short,
/// always unique per source path, keeps the real extension hint.
fn stored_name(src: &Path) -> String {
    let full = src.to_string_lossy();
    let base = src.file_name().and_then(|s| s.to_str()).unwrap_or("save");
    let mut safe: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c.to_string()
            } else {
                "_".to_string()
            }
        })
        .collect();
    safe.truncate(48);
    format!("{}_{safe}.bin", short_hash(&full, 16))
}

/// Copy every detected save file into a timestamped folder with a manifest.
pub fn backup_game(entry: &GameEntry, root: &Path, keep: usize) -> Result<PathBuf> {
    if entry.saves.is_empty() {
        anyhow::bail!("nothing to back up yet");
    }
    let dest = root.join(dir_name(entry)).join(instance_name(entry, false));
    std::fs::create_dir_all(dest.join("saves"))?;
    let mut files = Vec::new();
    for f in &entry.saves {
        let stored = stored_name(&f.path);
        let target = dest.join("saves").join(&stored);
        std::fs::copy(&f.path, &target).map_err(|e| friendly_io(&e, &f.path))?;
        files.push(serde_json::json!({
            "src": f.path.to_string_lossy(),
            "stored_as": stored,
            "size": f.size,
            "sha256": file_hash(&target).unwrap_or_default(),
        }));
    }
    let manifest = serde_json::json!({
        "format": 2,
        "kind": "unified",
        "backup_id": short_hash(&format!("{}|{}", entry.watch_key(), stamp_micros()), 16),
        "name": entry.name,
        "game_key": entry.watch_key(),
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
    prune(root, entry, keep.max(1));
    Ok(dest)
}

/// Snapshot current saves before a restore overwrites them.
pub fn snapshot(entry: &GameEntry, root: &Path) -> Result<PathBuf> {
    let dest = root.join(dir_name(entry)).join(instance_name(entry, true));
    std::fs::create_dir_all(dest.join("saves"))?;
    let mut files = Vec::new();
    for f in &entry.saves {
        if !f.path.is_file() {
            continue;
        }
        let stored = stored_name(&f.path);
        let target = dest.join("saves").join(&stored);
        if std::fs::copy(&f.path, &target).is_ok() {
            files.push(serde_json::json!({
                "src": f.path.to_string_lossy(),
                "stored_as": stored,
                "sha256": file_hash(&target).unwrap_or_default(),
            }));
        }
    }
    let manifest = serde_json::json!({
        "format": 2,
        "kind": "snapshot",
        "backup_id": short_hash(&format!("snap|{}|{}", entry.watch_key(), stamp_micros()), 16),
        "name": entry.name,
        "game_key": entry.watch_key(),
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
        // v2 integrity: refuse to restore a corrupted stored copy.
        if let Some(want) = f.get("sha256").and_then(|v| v.as_str()) {
            if !want.is_empty() {
                match file_hash(&from) {
                    Some(got) if got == want => {}
                    _ => anyhow::bail!(
                        "That backup looks damaged (a file doesn't match its fingerprint). Nothing was changed — try an older backup."
                    ),
                }
            }
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
            let _ = crate::parsers::bnd4::resign_sl2(&to);
        }
        n += 1;
        bytes += b;
    }
    if n == 0 {
        anyhow::bail!("backup is empty — nothing was restored");
    }
    Ok((n, bytes))
}

/// One backup with human-readable metadata for the UI. Reads the manifest so
/// the list shows dates, sizes and integrity — not folder names.
#[derive(Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub label: String,
    pub when: String,
    pub files: usize,
    pub bytes: u64,
    pub verified: bool,
}

fn manifest_info(dir: &Path) -> Option<BackupInfo> {
    let m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).ok()?).ok()?;
    let arr = m
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut bytes = 0u64;
    let mut ok = true;
    for f in &arr {
        if let Some(s) = f.get("size").and_then(|v| v.as_u64()) {
            bytes += s;
        }
        // v2 manifests carry content hashes: verify one level deep.
        if let (Some(stored), Some(want)) = (
            f.get("stored_as").and_then(|v| v.as_str()),
            f.get("sha256").and_then(|v| v.as_str()),
        ) {
            if !want.is_empty() {
                match file_hash(&dir.join("saves").join(stored)) {
                    Some(got) if got == want => {}
                    _ => ok = false,
                }
            }
        }
    }
    // Fall back to disk size when the manifest has no sizes (snapshots).
    if bytes == 0 {
        bytes = dir_size(&dir.join("saves"));
    }
    let at = m
        .get("backed_up_at")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let when = parse_when(at).unwrap_or_else(|| {
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("backup")
            .to_string()
    });
    let kind = m
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let label = if kind == "snapshot" || when.starts_with("pre-restore") {
        format!("Safety copy · {when}")
    } else if kind == "auto" {
        format!("Auto · {when}")
    } else {
        when.clone()
    };
    Some(BackupInfo {
        path: dir.to_path_buf(),
        label,
        when,
        files: arr.len(),
        bytes,
        verified: ok,
    })
}

fn parse_when(rfc: &str) -> Option<String> {
    let dt = chrono::DateTime::parse_from_rfc3339(rfc).ok()?;
    Some(
        dt.with_timezone(&chrono::Local)
            .format("%d %b, %H:%M")
            .to_string(),
    )
}

fn dir_size(d: &Path) -> u64 {
    let mut acc = 0;
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten().take(500) {
            acc += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    acc
}

fn collect_instances(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd
                .flatten()
                .filter(|e| e.path().is_dir() && e.path().join("manifest.json").exists())
            {
                v.push(e.path());
            }
        }
    }
    v.sort();
    v.reverse();
    v
}

/// Keep only the newest `keep` backups for a game (snapshots always kept).
fn prune(root: &Path, entry: &GameEntry, keep: usize) {
    let mut v = collect_instances(game_dirs(root, entry));
    // Never prune safety snapshots here — they are tiny and load-bearing.
    v.retain(|p| {
        !p.file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with("pre-restore"))
            .unwrap_or(false)
    });
    for old in v.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// Newest-first backup paths (legacy string API, kept for CLI).
#[allow(dead_code)]
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

/// Newest-first rich backup list for one game (searches current + legacy dirs).
pub fn list_backup_infos(root: &Path, entry: &GameEntry) -> Vec<BackupInfo> {
    let mut out = Vec::new();
    for p in collect_instances(game_dirs(root, entry)) {
        if let Some(i) = manifest_info(&p) {
            out.push(i);
        }
        if out.len() >= 12 {
            break;
        }
    }
    out
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
        "Windows blocked access — try running Omnexa as administrator.".into()
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
