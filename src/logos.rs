//! Game art, found automatically — never bundled, never guessed.
//!
//! Order per game: Steam library header (crisp, free, local) → SteamGridDB
//! grid (needs a free user key in Settings) → the game's own exe icon
//! (always available) → letter bauble fallback in the UI.
//! Everything lands in `<backup root>\cache\logos\`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::games::GameEntry;

fn cache_dir(backup_root: &Path) -> PathBuf {
    backup_root.join("cache").join("logos")
}

fn stem(entry: &GameEntry) -> String {
    entry
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_string()
            } else if c == ' ' {
                "-".to_string()
            } else {
                String::new()
            }
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Already-cached art for this game, if any.
pub fn find_cached(backup_root: &Path, entry: &GameEntry) -> Option<PathBuf> {
    let dir = cache_dir(backup_root);
    let s = stem(entry);
    for ext in ["png", "jpg", "jpeg", "webp"] {
        let p = dir.join(format!("{s}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Steam `appmanifest_*.acf` → installdir → appid, across all libraries.
fn steam_appids() -> HashMap<String, String> {
    let mut out = HashMap::new();
    for lib in crate::games::steam_library_dirs() {
        if let Ok(rd) = std::fs::read_dir(&lib) {
            for e in rd.flatten().take(200) {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("acf") {
                    continue;
                }
                if let Ok(t) = std::fs::read_to_string(&p) {
                    let mut appid = String::new();
                    let mut installdir = String::new();
                    let mut name = String::new();
                    for line in t.lines() {
                        let parts: Vec<&str> =
                            line.split('"').filter(|s| !s.trim().is_empty()).collect();
                        if parts.len() >= 2 {
                            match parts[0].trim() {
                                "appid" => appid = parts[1].trim().to_string(),
                                "installdir" => installdir = parts[1].trim().to_string(),
                                "name" => name = parts[1].trim().to_string(),
                                _ => {}
                            }
                        }
                    }
                    if !appid.is_empty() && !installdir.is_empty() {
                        out.insert(installdir.to_ascii_lowercase(), appid.clone());
                        if !name.is_empty() {
                            out.insert(name.to_ascii_lowercase(), appid);
                        }
                    }
                }
            }
        }
    }
    out
}

fn steam_cache_dirs() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam\appcache\librarycache"),
        PathBuf::from(r"C:\Program Files\Steam\appcache\librarycache"),
    ];
    for lib in crate::games::steam_library_dirs() {
        if let Some(steam_root) = lib.parent() {
            let p = steam_root.join("appcache").join("librarycache");
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

fn steam_header(
    entry: &GameEntry,
    appids: &HashMap<String, String>,
    dir: &Path,
) -> Option<PathBuf> {
    let folder = entry
        .root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let appid = appids
        .get(&folder)
        .or_else(|| appids.get(&entry.name.to_ascii_lowercase()))?;
    std::fs::create_dir_all(dir).ok()?;
    for file in [
        format!("{appid}_header.jpg"),
        format!("{appid}_library_600x900.jpg"),
    ] {
        for cache in steam_cache_dirs() {
            let src = cache.join(&file);
            if src.is_file() {
                let dst = dir.join(format!("{}.jpg", stem(entry)));
                if std::fs::copy(&src, &dst).is_ok() {
                    return Some(dst);
                }
            }
        }
    }
    None
}

fn steamgriddb(agent: &ureq::Agent, entry: &GameEntry, key: &str, dir: &Path) -> Option<PathBuf> {
    if key.trim().is_empty() {
        return None;
    }
    let auth = format!("Bearer {}", key.trim());
    let q: String = entry
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect();
    let search: serde_json::Value = agent
        .get(&format!(
            "https://www.steamgriddb.com/api/v2/search/autocomplete/{}",
            url_encode(&q)
        ))
        .set("Authorization", &auth)
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let id = search
        .get("data")?
        .as_array()?
        .first()?
        .get("id")?
        .as_u64()?;
    // Landscape headers fit the UI best; any grid will do.
    let grids: serde_json::Value = agent
        .get(&format!(
            "https://www.steamgriddb.com/api/v2/games/id/{id}/grids?dimensions=460x215,600x900&limit=1"
        ))
        .set("Authorization", &auth)
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let url = grids
        .get("data")?
        .as_array()?
        .first()?
        .get("url")?
        .as_str()?;
    let mut reader = agent.get(url).call().ok()?.into_reader();
    let mut bytes = Vec::new();
    use std::io::Read;
    reader.read_to_end(&mut bytes).ok()?;
    if bytes.len() < 1024 {
        return None;
    }
    let ext = url
        .rsplit('.')
        .next()
        .filter(|e| ["png", "jpg", "jpeg", "webp"].contains(e))
        .unwrap_or("jpg");
    std::fs::create_dir_all(dir).ok()?;
    let dst = dir.join(format!("{}.{}", stem(entry), ext));
    std::fs::write(&dst, &bytes).ok()?;
    Some(dst)
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b' ' {
            out.push(if b == b' ' { '+' } else { b as char });
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The exe's own icon via Windows (always available, small but real art).
fn exe_icon(entry: &GameEntry, dir: &Path) -> Option<PathBuf> {
    let exe = entry.exe.as_ref()?;
    std::fs::create_dir_all(dir).ok()?;
    let dst = dir.join(format!("{}.png", stem(entry)));
    let script = format!(
        "Add-Type -AssemblyName System.Drawing; $i=[System.Drawing.Icon]::ExtractAssociatedIcon('{}'); $b=$i.ToBitmap(); $b.Save('{}',[System.Drawing.Imaging.ImageFormat]::Png); $i.Dispose(); $b.Dispose()",
        exe.to_string_lossy().replace('\'', "''"),
        dst.to_string_lossy().replace('\'', "''")
    );
    let r = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if r.status.success() && dst.is_file() {
        Some(dst)
    } else {
        None
    }
}

/// Fill the logo cache for freshly scanned games. Runs on the scan thread.
pub fn ensure_all(backup_root: &Path, entries: &[GameEntry], sgdb_key: &str) {
    let dir = cache_dir(backup_root);
    let appids = steam_appids();
    let agent: ureq::Agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(15))
        .build();
    for e in entries {
        if find_cached(backup_root, e).is_some() {
            continue;
        }
        if steam_header(e, &appids, &dir).is_some() {
            continue;
        }
        if steamgriddb(&agent, e, sgdb_key, &dir).is_some() {
            continue;
        }
        let _ = exe_icon(e, &dir);
    }
}
