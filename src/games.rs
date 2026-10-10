//! Universal game discovery: no hardcoded game list.
//!
//! Installs come from Steam libraries, Ubisoft/Epic/GOG folders, the Windows
//! uninstall registry, and plain disk roots. Saves are matched by folder-name
//! heuristics plus in-install save dirs. FromSoftware formats are recognised
//! by file signature, everything else is treated as opaque files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct DetectedFile {
    pub path: PathBuf,
    pub size: u64,
    /// Last-modified unix time, for "played X ago".
    pub mtime: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    Sekiro,
    EldenRing,
    Nightreign,
}

pub fn fs_exes(kind: FsKind) -> &'static [&'static str] {
    match kind {
        FsKind::Sekiro => &["sekiro.exe"],
        FsKind::EldenRing => &["eldenring.exe"],
        FsKind::Nightreign => &["nightreign.exe"],
    }
}

#[derive(Clone)]
pub struct GameEntry {
    pub name: String,
    pub root: PathBuf,
    pub exe: Option<PathBuf>,
    pub source: &'static str,
    pub save_dirs: Vec<PathBuf>,
    pub saves: Vec<DetectedFile>,
    pub total_bytes: u64,
    pub fs: Option<FsKind>,
    pub progress: String,
    /// Newest save mtime (unix secs): "last played".
    pub last_played: Option<u64>,
}

impl GameEntry {
    /// Exe file names (lowercase) identifying this game while it runs.
    pub fn exe_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(e) = &self.exe {
            if let Some(n) = e.file_name().and_then(|s| s.to_str()) {
                v.push(n.to_ascii_lowercase());
            }
        }
        if let Some(fs) = self.fs {
            v.extend(fs_exes(fs).iter().map(|s| s.to_string()));
        }
        v.sort();
        v.dedup();
        v
    }

    /// Stable watch key for settings.
    pub fn watch_key(&self) -> String {
        let exe = self
            .exe
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        format!("{}|{}", self.root.to_string_lossy(), exe)
    }
}

// ---------------- save-file scanning ----------------

const SKIP_DIRS: [&str; 14] = [
    "Logs",
    "CrashReportClient",
    "CohCache",
    "Demos",
    "Dists",
    "IntegrityCheck",
    "node_modules",
    "Code",
    "npm",
    "Docker",
    "Unity",
    "Unreal Engine",
    "Visual Studio Setup",
    "Mozilla",
];

/// Collect save-ish files under a dir (bounded).
pub fn scan_files(dir: &Path, max: usize) -> Vec<DetectedFile> {
    let mut out = Vec::new();
    collect(dir, &mut out, 0, max);
    out.sort_by_key(|f| f.path.clone());
    out
}

fn interesting(path: &Path) -> bool {
    // Display set: save-ish extensions. No .json (config/telemetry spam),
    // no .log; see strong_save() for what proves a game.
    match path.extension().and_then(|s| s.to_str()) {
        Some(e) => {
            let e = e.to_ascii_lowercase();
            matches!(
                e.as_str(),
                "sl2"
                    | "sav"
                    | "save"
                    | "dat"
                    | "xml"
                    | "bin"
                    | "cfg"
                    | "ini"
                    | "bak"
                    | "bak1"
                    | "sav2"
                    | "profile"
                    | "zip"
                    | "wld"
                    | "plr"
                    | "pla"
                    | "rxdata"
                    | "es3"
            ) || path.to_string_lossy().ends_with(".sl2.bak")
        }
        None => {
            let n = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n == "save" || n == "save_backup" || n == "gamerprofile" || n.starts_with("USER_DATA")
        }
    }
}

fn collect(dir: &Path, out: &mut Vec<DetectedFile>, depth: usize, max: usize) {
    if depth > 5 || out.len() >= max {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        if out.len() >= max {
            return;
        }
        let p = e.path();
        if p.is_file() {
            if interesting(&p) {
                if let Ok(m) = e.metadata() {
                    if m.len() < 200_000_000 {
                        let mtime = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs());
                        out.push(DetectedFile {
                            path: p,
                            size: m.len(),
                            mtime,
                        });
                    }
                }
            }
        } else if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            collect(&p, out, depth + 1, max);
        }
    }
}

// ---------------- install discovery ----------------

/// Install roots that are tools/configs, never games.
const AUTO_BLOCK: [&str; 10] = [
    "steam controller configs",
    "steamworks shared",
    "steamworks",
    "easyanticheat",
    "_redist",
    "directx",
    "steam config",
    "controller configs",
    "openvr",
    "steamvr",
];

/// Registry/StartMenu junk that is never a game.
const NON_GAME: [&str; 24] = [
    "redistributable",
    "runtime",
    "sdk",
    "driver",
    "update for",
    "security update",
    "hotfix",
    "installer",
    "supportassist",
    "maxx audio",
    "elan touchpad",
    "intellitrace",
    "clickonce",
    "bootstrapper",
    "targeting pack",
    "workload",
    "toolset",
    "host fx",
    "apphost",
    "asp.net",
    "clr types",
    "diagnostics",
    "icecap",
    "vcpp",
];

/// Launcher apps: real programs, never games.
const LAUNCHERS: [&str; 6] = [
    "steam",
    "ubisoft connect",
    "gog galaxy",
    "epic games launcher",
    "ea app",
    "riot client",
];

/// Non-game software that still has exes + config folders.
/// (A junk filter — game discovery itself stays fully automatic.)
const JUNK: [&str; 34] = [
    "discord",
    "telegram",
    "docker",
    "nvidia",
    "vscode",
    "visual studio",
    "python",
    "roblox",
    "blender",
    "mongodb",
    "ollama",
    "winrar",
    "github",
    "obsidian",
    "vlc",
    "tailscale",
    "powertoys",
    "nodejs",
    "qbittorrent",
    "microsoft edge",
    "onedrive",
    "intel",
    "dell",
    "realtek",
    "elan",
    "waves audio",
    "teams",
    "zoom",
    "spotify",
    "adobe",
    "quadspinner",
    "downloader",
    "comfyui",
    "fl cloud",
];

/// Lowercase alphanumeric only — "Watch_Dogs 2" == "watch dogs2".
fn norm(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
const GENERIC_DIRS: [&str; 12] = [
    "common",
    "games",
    "steam",
    "ubisoft",
    "my games",
    "config",
    "saves",
    "savegames",
    "settings",
    "steam_settings",
    "profiles",
    "data",
];

fn roaming(sub: &str) -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| r"C:\Users\A3\AppData\Roaming".into());
    PathBuf::from(base).join(sub)
}

fn localdata(sub: &str) -> PathBuf {
    let base =
        std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"C:\Users\A3\AppData\Local".into());
    PathBuf::from(base).join(sub)
}

fn locallow(sub: &str) -> PathBuf {
    let base = dirs::data_dir()
        .and_then(|p| p.parent().map(|q| q.join("LocalLow")))
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\A3\AppData\LocalLow"));
    base.join(sub)
}

fn docs(sub: &str) -> PathBuf {
    let base = dirs::document_dir().unwrap_or_else(|| PathBuf::from(r"C:\Users\A3\Documents"));
    base.join(sub)
}

/// Pretty display name from a folder or exe-ish name.
pub fn prettify(raw: &str) -> String {
    let mut s = raw.replace(['_', '-', '.'], " ");
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    s.trim().to_string()
}

fn is_unins(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("unins")
}

/// First real exe in a dir, searching subdirs too (bounded).
/// Breadth-first so shallow launchers win over deep engine trees.
pub fn find_exe(root: &Path) -> Option<PathBuf> {
    use std::collections::VecDeque;
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(root.to_path_buf(), 0)]);
    let mut seen_dirs = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        if depth > 3 || seen_dirs > 400 {
            continue;
        }
        seen_dirs += 1;
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
                    if n.to_ascii_lowercase().ends_with(".exe") && !is_unins(n) {
                        return Some(p);
                    }
                }
            } else if p.is_dir() && depth < 3 {
                // Skip obvious non-game subtrees.
                let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !["_Redist", "redist", "DirectX", "__redist", "Engine"].contains(&n) {
                    queue.push_back((p, depth + 1));
                }
            }
        }
    }
    None
}

fn steam_libraries() -> Vec<PathBuf> {
    let mut libs = vec![
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps"),
        PathBuf::from(r"C:\Program Files\Steam\steamapps"),
    ];
    for drive in ["C", "D", "E", "F"] {
        for tail in [
            "SteamLibrary\\steamapps",
            "Steam\\steamapps",
            "Games\\SteamLibrary\\steamapps",
        ] {
            libs.push(PathBuf::from(format!("{drive}:\\{tail}")));
        }
    }
    for vdf in [
        r"C:\Program Files (x86)\Steam\steamapps\libraryfolders.vdf",
        r"C:\Program Files\Steam\steamapps\libraryfolders.vdf",
    ] {
        if let Ok(text) = std::fs::read_to_string(vdf) {
            for chunk in text.split('"') {
                let c = chunk.trim();
                if c.len() > 8 && (c.ends_with("steamapps") || c.ends_with(r"steamapps\common")) {
                    let p = PathBuf::from(c.replace("\\\\", "\\"));
                    let apps = if c.ends_with("common") {
                        p.parent().unwrap_or(&p).to_path_buf()
                    } else {
                        p
                    };
                    if !libs.contains(&apps) {
                        libs.push(apps);
                    }
                }
            }
        }
    }
    libs.into_iter().filter(|p| p.is_dir()).collect()
}

/// Steam library roots (shared with the logo finder).
pub fn steam_library_dirs() -> Vec<PathBuf> {
    steam_libraries()
}

/// Epic launcher manifests: exact (name, install dir) pairs.
fn epic_manifests() -> Vec<(String, PathBuf, &'static str)> {
    let mut out = Vec::new();
    let dir = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten().take(100) {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("item") {
                continue;
            }
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                    let name = v
                        .get("DisplayName")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let loc = v
                        .get("InstallLocation")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    if !name.is_empty() && !loc.is_empty() && PathBuf::from(loc).is_dir() {
                        out.push((name, PathBuf::from(loc), "Epic"));
                    }
                }
            }
        }
    }
    out
}

/// Windows uninstall registry: exact (name, install dir) pairs.
fn registry_installs() -> Vec<(String, PathBuf, &'static str)> {
    let mut out = Vec::new();
    let hives = [
        (
            winreg::enums::HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            winreg::enums::HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            winreg::enums::HKEY_CURRENT_USER,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];
    for (hive, base) in hives {
        let hk = match winreg::RegKey::predef(hive).open_subkey(base) {
            Ok(k) => k,
            Err(_) => continue,
        };
        for key in hk.enum_keys().flatten().take(500) {
            let sk = match hk.open_subkey(&key) {
                Ok(k) => k,
                Err(_) => continue,
            };
            let name: String = match sk.get_value::<String, _>("DisplayName") {
                Ok(n) if !n.trim().is_empty() => n,
                _ => continue,
            };
            let loc: String = match sk.get_value::<String, _>("InstallLocation") {
                Ok(l) => l,
                Err(_) => continue,
            };
            let dir = PathBuf::from(loc.trim());
            if !dir.is_dir() {
                continue;
            }
            let low = name.to_ascii_lowercase();
            if NON_GAME.iter().any(|b| low.contains(b)) {
                continue;
            }
            out.push((name, dir, "Registry"));
        }
    }
    out
}

/// Match install `name` (+ its exe stem, the UE4/Unity pattern) to save homes.
/// Compares normalised names so "Watch_Dogs 2" finds "Watch Dogs2", and
/// descends one level so "Hollow Knight" finds LocalLow\Team Cherry's child.
fn save_dirs_for(name: &str, exe_stem: Option<&str>) -> Vec<PathBuf> {
    let thelname = norm(name);
    let lowered = name.to_ascii_lowercase();
    let words: Vec<&str> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 5 && !GENERIC_DIRS.contains(w))
        .collect();
    let mut v = Vec::new();
    // UE4/Unity pattern: %LOCALAPPDATA%\<ExeName>, %APPDATA%\<ExeName>.
    if let Some(stem) = exe_stem {
        if stem.len() >= 3 && !is_unins(stem) {
            for base in [localdata(stem), roaming(stem)] {
                if base.is_dir() {
                    v.push(base);
                }
            }
            // ...and LocalLow\<anything>\<ExeName>.
            if let Ok(rd) = std::fs::read_dir(locallow("")) {
                for e in rd.flatten().take(100) {
                    let p = e.path().join(stem);
                    if p.is_dir() {
                        v.push(p);
                    }
                }
            }
        }
    }
    if !words.is_empty() || !thelname.is_empty() {
        // Score each candidate dir by matched words; keep only the best.
        // "Hollow Knight Silksong" matches its own folder 3/3 and plain
        // "Hollow Knight" 2/3, so each game keeps its own saves.
        let mut scored: Vec<(PathBuf, usize)> = Vec::new();
        let score_of = |dirname: &str| -> usize {
            let nd = norm(dirname);
            if nd.is_empty() {
                return 0;
            }
            let mut s = words.iter().filter(|w| nd.contains(&norm(w))).count();
            if thelname.contains(&nd) {
                s += 2;
            }
            s
        };
        let homes: Vec<PathBuf> = vec![
            roaming(""),
            localdata(""),
            locallow(""),
            docs(""),
            docs("My Games"),
        ];
        for home in homes {
            if let Ok(rd) = std::fs::read_dir(&home) {
                for e in rd.flatten().take(200) {
                    let p = e.path();
                    if !p.is_dir() {
                        continue;
                    }
                    let d = p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if d.is_empty() || GENERIC_DIRS.contains(&d.as_str()) {
                        continue;
                    }
                    let s = score_of(&d);
                    if s > 0 {
                        scored.push((p.clone(), s + 1));
                        continue;
                    }
                    // One level down: "Team Cherry" -> "Hollow Knight".
                    if let Ok(rd2) = std::fs::read_dir(&p) {
                        for c in rd2.flatten().take(50) {
                            let q = c.path();
                            if !q.is_dir() {
                                continue;
                            }
                            let cd = q.file_name().and_then(|s| s.to_str()).unwrap_or("");
                            if cd.is_empty() {
                                continue;
                            }
                            let cs = score_of(cd);
                            if cs > 0 {
                                scored.push((q, cs));
                            }
                        }
                    }
                }
            }
        }
        if let Some(best) = scored.iter().map(|(_, s)| *s).max() {
            for (p, s) in scored {
                if s == best {
                    v.push(p);
                }
            }
        }
    }
    // Engine/launcher homes keyed by well-known folder names.
    for key in [
        "Obscure",
        "TslGame",
        "CAPCOM",
        "Sekiro",
        "EldenRing",
        "Nightreign",
    ] {
        if name
            .to_ascii_lowercase()
            .contains(&key.to_ascii_lowercase())
        {
            for base in [localdata(key), roaming(key)] {
                if base.is_dir() {
                    v.push(base);
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v.truncate(6);
    v
}

/// Full universal discovery. `custom` are user-added folders from settings.
pub fn discover(custom: &[PathBuf]) -> Vec<GameEntry> {
    // (pretty name, root, source)
    let mut cands: Vec<(String, PathBuf, &'static str)> = Vec::new();

    for lib in steam_libraries() {
        let common = lib.join("common");
        if let Ok(rd) = std::fs::read_dir(&common) {
            for e in rd.flatten().take(80) {
                if e.path().is_dir() {
                    let n = e.file_name().to_string_lossy().to_string();
                    cands.push((prettify(&n), e.path(), "Steam"));
                }
            }
        }
    }
    let ubi = PathBuf::from(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games");
    if ubi.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&ubi) {
            for e in rd.flatten().take(40) {
                if e.path().is_dir() {
                    let n = e.file_name().to_string_lossy().to_string();
                    cands.push((prettify(&n), e.path(), "Ubisoft"));
                }
            }
        }
    }
    for drive in ["C", "D", "E"] {
        for tail in ["Epic Games", "GOG Games", "GOG Galaxy\\Games"] {
            let base = PathBuf::from(format!("{drive}:\\{tail}"));
            if drive == "C" && tail == "Epic Games" {
                for pf in [PathBuf::from(r"C:\Program Files\Epic Games"), base.clone()] {
                    if let Ok(rd) = std::fs::read_dir(&pf) {
                        for e in rd.flatten().take(60) {
                            if e.path().is_dir() {
                                let n = e.file_name().to_string_lossy().to_string();
                                cands.push((prettify(&n), e.path(), "Epic/GOG"));
                            }
                        }
                    }
                }
            } else if let Ok(rd) = std::fs::read_dir(&base) {
                for e in rd.flatten().take(60) {
                    if e.path().is_dir() {
                        let n = e.file_name().to_string_lossy().to_string();
                        cands.push((prettify(&n), e.path(), "Epic/GOG"));
                    }
                }
            }
        }
    }
    cands.extend(epic_manifests());
    cands.extend(registry_installs());
    // Plain disk roots (FitGirl-style C:\Game folders).
    for top in ["C:\\", "D:\\"] {
        if let Ok(rd) = std::fs::read_dir(top) {
            for e in rd.flatten().take(80) {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if n.len() < 3
                    || [
                        "Windows",
                        "Program Files",
                        "Program Files (x86)",
                        "Users",
                        "Intel",
                        "PerfLogs",
                        "inetpub",
                        "ProgramData",
                        "Recovery",
                        "System Volume Information",
                        "$Recycle.Bin",
                        "MSOCache",
                        "Documents and Settings",
                        "Config.Msi",
                    ]
                    .contains(&n)
                {
                    continue;
                }
                if find_exe(&p).is_some() {
                    cands.push((prettify(n), p, "This PC"));
                }
            }
        }
    }
    for c in custom {
        if c.is_dir() {
            let n = c.file_name().and_then(|s| s.to_str()).unwrap_or("Custom");
            cands.push((prettify(n), c.clone(), "Custom"));
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (name, root, source) in cands {
        let key = root.to_string_lossy().to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let lname = name.to_ascii_lowercase();
        if AUTO_BLOCK.iter().any(|b| lname.contains(b)) || name.len() < 2 {
            continue;
        }
        // Launchers and everyday non-game software are never games,
        // no matter what exes or folders they have. (Custom folders the
        // user added explicitly are always respected.)
        // Single-word launchers match exactly ("SteamWorld Dig" is a game);
        // multi-word ones match by containment.
        let is_launcher = LAUNCHERS.iter().any(|l| {
            if l.contains(' ') {
                lname.contains(l)
            } else {
                lname == *l
            }
        });
        if source != "Custom" && (is_launcher || JUNK.iter().any(|j| lname.contains(j))) {
            continue;
        }
        let exe = find_exe(&root);
        let exe_stem = exe
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let mut save_dirs = save_dirs_for(&name, exe_stem.as_deref());
        for extra in [
            "steam_settings",
            "savegames",
            "saves",
            "SaveGames",
            "SAVEDGAMES",
            "Profiles",
        ] {
            let p = root.join(extra);
            if p.is_dir() && !save_dirs.contains(&p) {
                save_dirs.push(p);
            }
        }
        let mut saves = Vec::new();
        for sd in &save_dirs {
            saves.extend(scan_files(sd, 60));
            if saves.len() > 80 {
                break;
            }
        }
        saves.sort_by_key(|f| f.path.clone());
        saves.truncate(60);
        if exe.is_none() && saves.is_empty() {
            continue;
        }
        // Bare disk folders with no saves are noise (code, downloads…).
        // Real installs come with their own source label anyway.
        if source == "This PC" && saves.is_empty() {
            continue;
        }
        // Registry entries are untrusted: drivers and tools all have exes
        // AND config folders. Without a real save file they are not games.
        let trusted = source != "Registry";
        if !trusted && !saves.iter().any(strong_save) {
            continue;
        }
        let total_bytes = saves.iter().map(|f| f.size).sum();
        let last_played = saves.iter().filter_map(|f| f.mtime).max();
        let fs = detect_fs(&saves);
        let progress = progress_line(fs, &saves);
        out.push(GameEntry {
            name,
            root,
            exe,
            source,
            save_dirs,
            saves,
            total_bytes,
            fs,
            progress,
            last_played,
        });
        if out.len() > 160 {
            break;
        }
    }
    out.extend(orphan_saves(&out));
    out = merge_duplicates(out);
    // Biggest saves first — your real games float to the top, tools sink.
    out.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    out.truncate(120);
    out
}

/// Save folders with real progress but no detected install
/// (uninstalled games, leftovers, portable saves).
fn orphan_saves(covered: &[GameEntry]) -> Vec<GameEntry> {
    let mut have: HashSet<String> = HashSet::new();
    for g in covered {
        for f in &g.saves {
            have.insert(f.path.to_string_lossy().to_ascii_lowercase());
        }
        for d in &g.save_dirs {
            have.insert(d.to_string_lossy().to_ascii_lowercase());
        }
    }
    let mut out = Vec::new();
    let homes = vec![
        roaming(""),
        localdata(""),
        locallow(""),
        docs(""),
        docs("My Games"),
    ];
    for home in homes {
        let rd = match std::fs::read_dir(&home) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten().take(200) {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let dirname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if dirname.is_empty() {
                continue;
            }
            let low = dirname.to_ascii_lowercase();
            if GENERIC_DIRS.contains(&low.as_str())
                || JUNK.iter().any(|j| low.contains(j))
                || have.contains(&p.to_string_lossy().to_ascii_lowercase())
            {
                continue;
            }
            let saves = scan_files(&p, 40);
            if !saves.iter().any(|f| orphan_strong(&p, f)) {
                continue;
            }
            // Partial overlap with a detected game means same saves, new label.
            if saves
                .iter()
                .any(|f| have.contains(&f.path.to_string_lossy().to_ascii_lowercase()))
            {
                continue;
            }
            let total_bytes = saves.iter().map(|f| f.size).sum();
            let last_played = saves.iter().filter_map(|f| f.mtime).max();
            let fs = detect_fs(&saves);
            let progress = progress_line(fs, &saves);
            out.push(GameEntry {
                name: prettify(dirname),
                root: p.clone(),
                exe: None,
                source: "Saves only",
                save_dirs: vec![p],
                saves,
                total_bytes,
                fs,
                progress,
                last_played,
            });
            if out.len() > 20 {
                return out;
            }
        }
    }
    out
}

/// A file that proves progress, not just settings.
/// Configs (.ini/.xml), telemetry, shaders and caches never qualify.
/// Generic `.dat` alone doesn't either — Discord and drivers are full of
/// those — only save-flavoured names (user1.dat, level.dat…) or
/// unambiguous formats do. `.zip` counts: Factorio and friends save zipped.
fn strong_save(f: &DetectedFile) -> bool {
    strict_strong(f)
        || f.path
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".zip")
}

/// Strict evidence for orphan save folders (no install found): zips live in
/// every Electron cache, so a zip only counts when its name says save
/// (Factorio autosaves). Loosened only under Documents\My Games.
fn orphan_strong(dir: &Path, f: &DetectedFile) -> bool {
    if strict_strong(f) {
        return true;
    }
    let slow = f.path.to_string_lossy().to_ascii_lowercase();
    if slow.ends_with(".zip") {
        if let Some(stem) = f.path.file_stem().and_then(|x| x.to_str()) {
            let st = stem.to_ascii_lowercase();
            if [
                "save", "slot", "auto", "world", "level", "ironman", "campaign", "profile",
                "backup", "chapter",
            ]
            .iter()
            .any(|k| st.contains(k))
            {
                return true;
            }
        }
    }
    if dir
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains("my games")
    {
        return strong_save(f);
    }
    false
}

fn strict_strong(f: &DetectedFile) -> bool {
    let s = f.path.to_string_lossy().to_ascii_lowercase();
    if s.ends_with(".sl2")
        || s.ends_with(".sl2.bak")
        || s.ends_with(".sav")
        || s.ends_with(".save")
        || s.ends_with(".bak1")
        || s.ends_with(".profile")
        || s.ends_with(".wld")
        || s.ends_with(".plr")
        || s.ends_with(".pla")
        || s.ends_with(".rxdata")
        || s.ends_with(".es3")
    {
        return true;
    }
    if let Some(n) = f.path.file_name().and_then(|x| x.to_str()) {
        let n = n.to_ascii_lowercase();
        if n == "save" || n == "save_backup" || n == "gamerprofile" {
            return true;
        }
        if n.ends_with(".dat") {
            // Save-flavoured stems only: user1.dat, level.dat… — but not
            // userdict_v1.dat (Windows IME) or userdata blobs. The char
            // after the prefix must not be a letter.
            let stem = &n[..n.len() - 4];
            for pre in ["user", "level", "player", "slot", "save", "world"] {
                if stem == pre
                    || stem
                        .strip_prefix(pre)
                        .map(|rest| {
                            rest.chars()
                                .next()
                                .map(|c| !c.is_ascii_alphabetic())
                                .unwrap_or(true)
                        })
                        .unwrap_or(false)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn source_prio(source: &str) -> u8 {
    match source {
        "Steam" => 0,
        "Epic" => 1,
        "Epic/GOG" => 1,
        "Ubisoft" => 2,
        "Registry" => 3,
        "This PC" => 4,
        "Custom" => 5,
        _ => 6,
    }
}

/// Two candidates pointing at the same saves are the same game
/// (registry "ELDEN RING" + disk "ELDEN RING"). Merge them: union of
/// saves, prettiest name, most trusted source.
fn merge_duplicates(mut entries: Vec<GameEntry>) -> Vec<GameEntry> {
    fn identity(e: &GameEntry) -> String {
        e.saves
            .iter()
            .filter(|f| strong_save(f))
            .max_by_key(|f| f.size)
            .map(|f| f.path.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| e.root.to_string_lossy().to_ascii_lowercase())
    }
    let mut out: Vec<GameEntry> = Vec::new();
    for e in entries.drain(..) {
        let key = identity(&e);
        if let Some(slot) = out.iter_mut().find(|o| identity(o) == key) {
            for f in &e.saves {
                if !slot.saves.iter().any(|x| x.path == f.path) {
                    slot.saves.push(f.clone());
                }
            }
            for d in &e.save_dirs {
                if !slot.save_dirs.contains(d) {
                    slot.save_dirs.push(d.clone());
                }
            }
            if e.name.len() > slot.name.len() {
                slot.name = e.name.clone();
            }
            if source_prio(e.source) < source_prio(slot.source) {
                slot.source = e.source;
            }
            if slot.exe.is_none() {
                slot.exe = e.exe.clone();
            }
            slot.saves.sort_by_key(|f| f.path.clone());
            slot.saves.truncate(60);
            slot.total_bytes = slot.saves.iter().map(|f| f.size).sum();
            slot.last_played = slot.saves.iter().filter_map(|f| f.mtime).max();
            let fs = detect_fs(&slot.saves);
            slot.fs = fs;
            slot.progress = progress_line(fs, &slot.saves);
        } else {
            out.push(e);
        }
    }
    out
}

/// Recognise a FromSoftware save by signature, never by install name.
fn detect_fs(saves: &[DetectedFile]) -> Option<FsKind> {
    let sl2 = saves.iter().find(|f| {
        f.path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("sl2"))
            == Some(true)
    })?;
    let stem = sl2
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    if stem.starts_with("S0000") {
        return Some(FsKind::Sekiro);
    }
    if stem.starts_with("ER0000") {
        return Some(FsKind::EldenRing);
    }
    if stem.starts_with("NR0000") {
        return Some(FsKind::Nightreign);
    }
    // Unknown name: sniff content. BND4 + a valid ER roster = Elden Ring.
    if let Ok(bytes) = std::fs::read(&sl2.path) {
        if bytes.len() > 0x40 && &bytes[0..4] == b"BND4" {
            if crate::elden::parse(&bytes)
                .map(|s| s.iter().any(|x| x.active))
                .unwrap_or(false)
            {
                return Some(FsKind::EldenRing);
            }
            return None;
        }
    }
    None
}

fn progress_line(fs: Option<FsKind>, found: &[DetectedFile]) -> String {
    if found.is_empty() {
        return "Installed · no saves yet".into();
    }
    match fs {
        Some(FsKind::Sekiro) => {
            if let Some(f) = found.iter().find(|f| {
                f.path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|e| e.eq_ignore_ascii_case("sl2"))
                    == Some(true)
            }) {
                if let Ok(bytes) = std::fs::read(&f.path) {
                    if let Ok(slots) = crate::sekiro::parse(&bytes) {
                        let used: Vec<&crate::sekiro::SekiroSlot> =
                            slots.iter().filter(|s| s.used).collect();
                        if used.is_empty() {
                            return "Save present · no characters yet".into();
                        }
                        if used.len() == 1 {
                            let s = used[0];
                            return format!(
                                "1 character · {} played",
                                crate::sekiro::fmt_playtime(s.playtime_secs)
                            );
                        }
                        return format!("{} characters", used.len());
                    }
                }
            }
            "Save present".into()
        }
        Some(FsKind::EldenRing) => {
            if let Some(f) = found.iter().find(|f| {
                f.path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|e| e.eq_ignore_ascii_case("sl2"))
                    == Some(true)
            }) {
                if let Ok(bytes) = std::fs::read(&f.path) {
                    if let Ok(slots) = crate::elden::parse(&bytes) {
                        let chars: Vec<String> = slots
                            .iter()
                            .filter(|s| s.active)
                            .map(|s| match &s.name {
                                Some(n) => format!("{n} (Lv {})", s.level),
                                None => format!("Lv {}", s.level),
                            })
                            .collect();
                        if chars.is_empty() {
                            return "Save present · no characters yet".into();
                        }
                        return chars.join(" · ");
                    }
                }
            }
            "Save present".into()
        }
        Some(FsKind::Nightreign) => "Save present · details not mapped yet".into(),
        None => {
            let n = found
                .iter()
                .filter(|f| {
                    let s = f.path.to_string_lossy().to_ascii_lowercase();
                    !(s.ends_with(".ini")
                        || s.ends_with(".xml")
                        || s.ends_with(".log")
                        || s.ends_with(".ctx")
                        || s.ends_with(".hdr"))
                })
                .count();
            if n == 0 {
                format!(
                    "Settings only · {}",
                    fmt_bytes(found.iter().map(|f| f.size).sum())
                )
            } else {
                format!(
                    "{} save file{} · {}",
                    n,
                    if n == 1 { "" } else { "s" },
                    fmt_bytes(found.iter().map(|f| f.size).sum::<u64>())
                )
            }
        }
    }
}

pub fn fmt_bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < 4 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", n, U[u])
    } else {
        format!("{:.1} {}", v, U[u])
    }
}

/// "played 3 h ago" from unix times.
pub fn fmt_relative(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    if d < 90 {
        "just now".into()
    } else if d < 5400 {
        format!("{} min ago", d / 60)
    } else if d < 86400 * 2 {
        format!("{} h ago", d / 3600)
    } else if d < 86400 * 30 {
        format!("{} d ago", d / 86400)
    } else {
        fmt_date(then)
    }
}

/// "12 Oct" from unix time (local).
pub fn fmt_date(t: u64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(t as i64, 0)
        .single()
        .map(|d| d.format("%d %b").to_string())
        .unwrap_or_default()
}

/// Current unix time.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
