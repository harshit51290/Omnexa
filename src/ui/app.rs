use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::core::games::{self, fmt_bytes, FsKind, GameEntry};
use crate::core::settings::Settings;
use crate::core::{backup, db, logos, settings};
use crate::parsers::{elden, gvas, hollow, minecraft, sekiro, shady};
use crate::ui::cards::*;
use crate::ui::theme::*;

#[derive(Clone)]
enum SaveView {
    Sekiro(Vec<sekiro::SekiroSlot>),
    Er(Vec<elden::ErSlot>),
    Hollow(Vec<(String, hollow::HollowSave)>),
    Ue(UeAggregate),
    Mc(Vec<(String, minecraft::McWorld)>),
    Shady(shady::ShadySave),
    Json {
        file: String,
        fields: Vec<(String, String)>,
        keys: usize,
    },
    Other,
}

enum ScanMsg {
    Done(Vec<GameEntry>),
}

pub(crate) struct App {
    games: Vec<GameEntry>,
    scanning: bool,
    scan_rx: mpsc::Receiver<ScanMsg>,
    scan_tx: mpsc::Sender<ScanMsg>,
    sel: Option<usize>,
    backup_root: PathBuf,
    settings: Settings,
    status: String,
    toasts: Vec<Toast>,
    search: String,
    filter: Filter,
    view: Nav,
    root_edit: String,
    selected: std::collections::HashSet<String>,
    delete_arm: Option<PathBuf>,
    events_rx: mpsc::Receiver<String>,
    backups: Vec<backup::BackupInfo>,
    views: HashMap<String, SaveView>,
    textures: HashMap<String, egui::TextureHandle>,
    running_cache: HashSetCache,
    busy: Option<String>,
    restore_pick: Option<PathBuf>,
    job_rx: mpsc::Receiver<JobMsg>,
    job_tx: mpsc::Sender<JobMsg>,
}

#[derive(Clone)]
struct Toast {
    text: String,
    at: Instant,
    good: bool,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Nav {
    #[default]
    Games,
    Backups,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Filter {
    #[default]
    All,
    Protected,
    NeedsBackup,
    Running,
}

struct JobMsg {
    text: String,
    good: bool,
    sel: Option<usize>,
}

#[derive(Default)]
struct HashSetCache {
    set: std::collections::HashSet<String>,
    at: Option<Instant>,
}

impl App {
    pub(crate) fn new(events_rx: mpsc::Receiver<String>) -> Self {
        let backup_root = backup::default_backup_root();
        let settings = settings::load(&backup_root);
        let backup_root = settings.backup_root.clone();
        let (scan_tx, scan_rx) = mpsc::channel();
        let (job_tx, job_rx) = mpsc::channel();
        let mut app = Self {
            games: Vec::new(),
            scanning: true,
            scan_rx,
            scan_tx,
            sel: None,
            backup_root,
            settings,
            status: "Looking for your games…".into(),
            toasts: Vec::new(),
            search: String::new(),
            filter: Filter::All,
            view: Nav::Games,
            root_edit: String::new(),
            selected: std::collections::HashSet::new(),
            delete_arm: None,
            events_rx,
            backups: Vec::new(),
            views: HashMap::new(),
            textures: HashMap::new(),
            running_cache: HashSetCache::default(),
            busy: None,
            restore_pick: None,
            job_rx,
            job_tx,
        };
        app.start_scan();
        app
    }

    fn start_scan(&mut self) {
        self.scanning = true;
        self.status = "Looking for your games…".into();
        let tx = self.scan_tx.clone();
        let custom = self.settings.custom_folders.clone();
        let backup_root = self.backup_root.clone();
        let sgdb_key = self.settings.sgdb_key.clone();
        std::thread::spawn(move || {
            let games = games::discover(&custom);
            // Cover art while we're already off the UI thread.
            logos::ensure_all(&backup_root, &games, &sgdb_key);
            let _ = tx.send(ScanMsg::Done(games));
        });
    }

    fn poll_scan(&mut self) {
        while let Ok(ScanMsg::Done(games)) = self.scan_rx.try_recv() {
            let n: usize = games.iter().map(|g| g.saves.len()).sum();
            db::record_scan(&self.backup_root, &games);
            self.games = games;
            self.scanning = false;
            if self.sel.map(|i| i >= self.games.len()).unwrap_or(true) {
                self.sel = self.games.iter().position(|g| !g.saves.is_empty());
            }
            self.refresh_backups();
            let msg = if n == 0 {
                "No saves found — add a folder in Settings if a game is missing.".to_string()
            } else {
                format!("Found {n} save files. Pick a game to protect it.")
            };
            self.toast(msg, true);
        }
    }

    fn toast(&mut self, text: impl Into<String>, good: bool) {
        let text = text.into();
        self.status = text.clone();
        self.toasts.push(Toast {
            text,
            at: Instant::now(),
            good,
        });
        self.toasts
            .retain(|t| t.at.elapsed() < Duration::from_secs(12));
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn poll_jobs(&mut self) {
        while let Ok(m) = self.job_rx.try_recv() {
            self.busy = None;
            self.toast(m.text, m.good);
            if let Some(i) = m.sel {
                if i < self.games.len() {
                    let mut g = self.games[i].clone();
                    backup::refresh_saves(&mut g);
                    self.games[i] = g;
                }
                if self.sel == Some(i) {
                    self.refresh_backups();
                }
                self.views.clear();
            } else {
                self.refresh_backups();
            }
        }
        self.toasts
            .retain(|t| t.at.elapsed() < Duration::from_secs(12));
    }

    fn refresh_backups(&mut self) {
        self.backups.clear();
        if let Some(i) = self.sel {
            if let Some(g) = self.games.get(i) {
                self.backups = backup::list_backup_infos(&self.backup_root, g);
            }
        }
    }

    fn protection(&self, g: &GameEntry) -> bool {
        // Protected = a real backup exists on disk (current hash dir or
        // legacy readable dir from pre-SHA-256 builds).
        for d in [
            self.backup_root.join(backup::dir_name(g)),
            self.backup_root.join(backup::legacy_dir_name(g)),
        ] {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten().take(30) {
                    if e.path().is_dir() && e.path().join("manifest.json").exists() {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn last_backup_line(&self) -> String {
        match self.backups.first() {
            Some(b) => format!("Last backup {}", b.when),
            None => "Not backed up yet".into(),
        }
    }

    fn running_now(&mut self) -> std::collections::HashSet<String> {
        let stale = self
            .running_cache
            .at
            .map(|t| t.elapsed() > Duration::from_secs(5))
            .unwrap_or(true);
        if stale {
            self.running_cache.set = settings::running_exes();
            self.running_cache.at = Some(Instant::now());
        }
        self.running_cache.set.clone()
    }

    fn save_view(&mut self, game: &GameEntry) -> SaveView {
        let key = game_key(game);
        if let Some(v) = self.views.get(&key) {
            return v.clone();
        }
        let v = inspect_game(game);
        self.views.insert(key, v.clone());
        v
    }

    fn poll_events(&mut self) {
        let mut hits = Vec::new();
        while let Ok(exe) = self.events_rx.try_recv() {
            hits.push(exe);
        }
        if hits.is_empty() || !self.settings.auto_backup {
            return;
        }
        for exe in hits {
            let low = exe.to_ascii_lowercase();
            let hit = self.games.iter().position(|g| {
                g.exe_names().iter().any(|n| n == &low)
                    && self
                        .settings
                        .watched
                        .iter()
                        .any(|w| w == &g.watch_key() || same_root(w, &g.watch_key()))
            });
            if let Some(i) = hit {
                let mut g = self.games[i].clone();
                backup::refresh_saves(&mut g);
                if g.saves.is_empty() {
                    continue;
                }
                let key = g.watch_key();
                let (nfiles, nbytes) = (g.saves.len(), g.total_bytes);
                match backup::backup_game(&g, &self.backup_root, self.settings.retention) {
                    Ok(dest) => {
                        db::record_backup(&self.backup_root, &key, &dest, "auto", nfiles, nbytes);
                        self.games[i] = g;
                        self.toast(
                            format!("Auto backup done for {} — you're safe.", self.games[i].name),
                            true,
                        );
                        if self.sel == Some(i) {
                            self.refresh_backups();
                        }
                    }
                    Err(e) => self.toast(backup::friendly_err(&e), false),
                }
            }
        }
    }

    fn do_backup(&mut self, i: usize) {
        if self.busy.is_some() || i >= self.games.len() {
            return;
        }
        let mut g = self.games[i].clone();
        backup::refresh_saves(&mut g);
        self.games[i] = g.clone();
        if g.saves.is_empty() {
            self.toast("Nothing to back up yet.", false);
            return;
        }
        self.busy = Some(format!("Backing up {}…", g.name));
        let tx = self.job_tx.clone();
        let root = self.backup_root.clone();
        let keep = self.settings.retention;
        let name = g.name.clone();
        std::thread::spawn(move || {
            let key = g.watch_key();
            let (nfiles, nbytes) = (g.saves.len(), g.total_bytes);
            match backup::backup_game(&g, &root, keep) {
                Ok(dest) => {
                    db::record_backup(&root, &key, &dest, "manual", nfiles, nbytes);
                    let _ = tx.send(JobMsg {
                        text: format!("Backed up {name} — relax, it's safe."),
                        good: true,
                        sel: Some(i),
                    });
                }
                Err(e) => {
                    let _ = tx.send(JobMsg {
                        text: backup::friendly_err(&e),
                        good: false,
                        sel: None,
                    });
                }
            }
        });
    }

    fn do_backup_batch(&mut self) {
        if self.busy.is_some() || self.selected.is_empty() {
            return;
        }
        let idx: Vec<usize> = self
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| self.selected.contains(&g.watch_key()))
            .map(|(i, _)| i)
            .collect();
        if idx.is_empty() {
            return;
        }
        self.busy = Some(format!("Backing up {} games…", idx.len()));
        self.selected.clear();
        let tx = self.job_tx.clone();
        let root = self.backup_root.clone();
        let keep = self.settings.retention;
        let games: Vec<GameEntry> = idx
            .into_iter()
            .map(|i| {
                let mut g = self.games[i].clone();
                backup::refresh_saves(&mut g);
                g
            })
            .collect();
        let cur = self.sel;
        std::thread::spawn(move || {
            let mut ok = 0;
            let mut fail = 0;
            for g in &games {
                if g.saves.is_empty() {
                    continue;
                }
                match backup::backup_game(g, &root, keep) {
                    Ok(dest) => {
                        db::record_backup(
                            &root,
                            &g.watch_key(),
                            &dest,
                            "manual",
                            g.saves.len(),
                            g.total_bytes,
                        );
                        ok += 1;
                    }
                    Err(_) => {
                        fail += 1;
                    }
                }
            }
            let _ = tx.send(JobMsg {
                text: if fail == 0 {
                    format!("Backed up {ok} games — all safe.")
                } else {
                    format!("Backed up {ok} games, {fail} failed — see their rows.")
                },
                good: fail == 0,
                sel: cur,
            });
        });
    }

    fn do_restore(&mut self, i: usize, backup_dir: PathBuf) {
        if self.busy.is_some() || i >= self.games.len() {
            return;
        }
        self.restore_pick = None;
        self.busy = Some("Restoring…".into());
        let tx = self.job_tx.clone();
        let root = self.backup_root.clone();
        let mut g = self.games[i].clone();
        std::thread::spawn(move || {
            backup::refresh_saves(&mut g);
            // Safety first: snapshot whatever is there now.
            if let Ok(snap) = backup::snapshot(&g, &root) {
                db::record_backup(
                    &root,
                    &g.watch_key(),
                    &snap,
                    "snapshot",
                    g.saves.len(),
                    g.total_bytes,
                );
            }
            let name = g.name.clone();
            match backup::restore_backup(&backup_dir) {
                Ok((n, _)) => {
                    let _ = tx.send(JobMsg {
                        text: format!(
                            "Restored {n} files for {name}. The previous state is kept as a safety copy."
                        ),
                        good: true,
                        sel: Some(i),
                    });
                }
                Err(e) => {
                    let _ = tx.send(JobMsg {
                        text: backup::friendly_err(&e),
                        good: false,
                        sel: None,
                    });
                }
            }
        });
    }
}

fn same_root(a: &str, b: &str) -> bool {
    let ra = a.split('|').next().unwrap_or("");
    let rb = b.split('|').next().unwrap_or("");
    !ra.is_empty() && ra.eq_ignore_ascii_case(rb)
}

/// Cache key: first save path + count (refreshes when files change).
fn game_key(game: &GameEntry) -> String {
    let first = game
        .saves
        .first()
        .map(|f| f.path.to_string_lossy().to_string())
        .unwrap_or_default();
    format!(
        "{}|{}|{}",
        game.root.to_string_lossy(),
        first,
        game.saves.len()
    )
}

fn file_name_of(p: &std::path::Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("save")
        .to_string()
}

fn fmt_when(p: &std::path::Path) -> String {
    if let Ok(m) = std::fs::metadata(p) {
        if let Ok(t) = m.modified() {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            return dt.format("%d %b, %H:%M").to_string();
        }
    }
    String::new()
}

/// Read a game entry into human words. Tries, in order: FromSoftware
/// details, Hollow Knight, Minecraft, Shady, Unreal, plain JSON.
fn inspect_game(game: &GameEntry) -> SaveView {
    let sl2 = game.saves.iter().find(|f| {
        f.path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("sl2"))
            == Some(true)
    });
    if let Some(f) = sl2 {
        if let Ok(bytes) = std::fs::read(&f.path) {
            if game.fs == Some(FsKind::Sekiro) {
                if let Ok(slots) = sekiro::parse(&bytes) {
                    return SaveView::Sekiro(slots);
                }
            }
            if game.fs == Some(FsKind::EldenRing) {
                if let Ok(slots) = elden::parse(&bytes) {
                    return SaveView::Er(slots);
                }
            }
        }
    }
    // Hollow Knight / Silksong: user*.dat
    let mut hk = Vec::new();
    for f in game.saves.iter().filter(|f| {
        f.path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("dat"))
            == Some(true)
            && f.path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.to_ascii_lowercase().starts_with("user"))
                == Some(true)
    }) {
        if let Ok(bytes) = std::fs::read(&f.path) {
            if let Ok(s) = hollow::parse(&bytes) {
                hk.push((file_name_of(&f.path), s));
            }
        }
        if hk.len() >= 4 {
            break;
        }
    }
    if !hk.is_empty() {
        return SaveView::Hollow(hk);
    }
    // Minecraft: level.dat
    let mut mc = Vec::new();
    for f in game.saves.iter().filter(|f| {
        f.path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.eq_ignore_ascii_case("level.dat"))
            == Some(true)
    }) {
        if let Ok(bytes) = std::fs::read(&f.path) {
            if let Ok(w) = minecraft::parse(&bytes) {
                let world = f
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                mc.push((world, w));
            }
        }
        if mc.len() >= 6 {
            break;
        }
    }
    if !mc.is_empty() {
        return SaveView::Mc(mc);
    }
    // Shady Part of Me: file literally named "save"
    for f in &game.saves {
        if f.path.file_name().and_then(|s| s.to_str()) == Some("save") {
            if let Ok(bytes) = std::fs::read(&f.path) {
                if let Ok(s) = shady::parse(&bytes) {
                    return SaveView::Shady(s);
                }
            }
            break;
        }
    }
    // Unreal .sav files (any UE game)
    let savs: Vec<&games::DetectedFile> = game
        .saves
        .iter()
        .filter(|f| {
            f.path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("sav"))
                == Some(true)
        })
        .collect();
    if !savs.is_empty() {
        let mut agg = UeAggregate::default();
        agg.files = savs.len();
        let mut ordered: Vec<(&games::DetectedFile, String)> =
            savs.iter().map(|f| (*f, fmt_when(&f.path))).collect();
        // most recently modified first — best effort without sorting by time
        for (f, _) in ordered.iter().take(40) {
            if agg.engine.is_empty() {
                if let Ok(bytes) = std::fs::read(&f.path) {
                    if let Ok(info) = gvas::parse(&bytes) {
                        agg.engine = info.engine;
                        agg.class = info
                            .class
                            .rsplit(['.', '/'])
                            .next()
                            .unwrap_or("")
                            .to_string();
                        agg.partial = info.partial;
                        agg.labels = info.labels;
                    }
                }
            }
        }
        let mut with_time: Vec<(&games::DetectedFile, std::time::SystemTime)> = savs
            .iter()
            .filter_map(|f| {
                std::fs::metadata(&f.path)
                    .ok()?
                    .modified()
                    .ok()
                    .map(|t| (*f, t))
            })
            .collect();
        with_time.sort_by(|a, b| b.1.cmp(&a.1));
        agg.recent = with_time
            .into_iter()
            .take(5)
            .map(|(f, _)| (file_name_of(&f.path), fmt_when(&f.path)))
            .collect();
        let _ = &mut ordered;
        return SaveView::Ue(agg);
    }
    // Plain JSON saves (indie games): show the interesting fields.
    for f in game.saves.iter().filter(|f| {
        f.path
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.eq_ignore_ascii_case("json"))
            == Some(true)
    }) {
        if let Ok(bytes) = std::fs::read(&f.path) {
            if bytes.len() < 2_000_000 {
                if let Some((fields, keys)) = json_preview(&bytes) {
                    return SaveView::Json {
                        file: file_name_of(&f.path),
                        fields,
                        keys,
                    };
                }
            }
        }
        break;
    }
    SaveView::Other
}

/// Friendly preview of a JSON save: scalar fields + total key count.
fn json_preview(bytes: &[u8]) -> Option<(Vec<(String, String)>, usize)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = v.as_object()?;
    let mut fields = Vec::new();
    for (k, val) in obj.iter() {
        let s = match val {
            serde_json::Value::String(s) if s.len() <= 60 => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Array(a) => format!("{} items", a.len()),
            serde_json::Value::Object(o) => format!("{} entries", o.len()),
            _ => continue,
        };
        let label = k
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        fields.push((label, s));
        if fields.len() >= 8 {
            break;
        }
    }
    if fields.is_empty() {
        return None;
    }
    Some((fields, obj.len()))
}

pub(crate) fn headless() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        // Full pipeline without a window: discover, art, database.
        let root = backup::default_backup_root();
        let settings = settings::load(&root);
        let games = games::discover(&settings.custom_folders);
        logos::ensure_all(&root, &games, &settings.sgdb_key);
        db::record_scan(&root, &games);
        for g in games.iter().take(25) {
            let logo = logos::find_cached(&root, g)
                .map(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_string()
                })
                .unwrap_or_else(|| "-".into());
            println!(
                "{} [{}] saves={} logo={}",
                g.name,
                g.source,
                g.saves.len(),
                logo
            );
        }
        if let Some(g) = games.first() {
            let h = db::history(&root, &g.watch_key());
            println!(
                "db: tracked_since={:?} backups={} history_lines={}",
                h.first_seen,
                h.backups,
                h.history.len()
            );
        }
        // End-to-end backup of the smallest game (safe: new timestamped folder).
        if let Some(g) = games
            .iter()
            .filter(|g| !g.saves.is_empty())
            .min_by_key(|g| g.total_bytes)
        {
            let key = g.watch_key();
            match backup::backup_game(g, &root, 8) {
                Ok(dest) => {
                    db::record_backup(&root, &key, &dest, "manual", g.saves.len(), g.total_bytes);
                    let h = db::history(&root, &key);
                    println!(
                        "backup: {} -> {} (backups now {})",
                        g.name,
                        dest.display(),
                        h.backups
                    );
                }
                Err(e) => println!("backup FAILED for {}: {e:#}", g.name),
            }
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--details") {
        if let Some(p) = args.get(i + 1) {
            return details_cmd(p);
        }
        println!("usage: omnexa --details <save file>");
        return Ok(());
    }
    for g in games::discover(&[]) {
        println!("{} [{}] | {}", g.name, g.source, g.progress);
    }
    Ok(())
}

/// Print everything readable in one save file. Used to verify the readers.
fn details_cmd(path: &str) -> eframe::Result<()> {
    let p = std::path::Path::new(path);
    let bytes = match std::fs::read(p) {
        Ok(b) => b,
        Err(e) => {
            println!("can't read: {e}");
            return Ok(());
        }
    };
    println!("file: {path} ({} bytes)", bytes.len());
    if bytes.len() > 4 && &bytes[..4] == b"BND4" {
        if let Ok(slots) = sekiro::parse(&bytes) {
            let used: Vec<_> = slots.iter().filter(|s| s.used).collect();
            if !used.is_empty() {
                println!("Sekiro save:");
                for s in used {
                    println!(
                        "  slot {}: {} · Sen {} · Attack {:?} · Vitality {:?} · HP {:?} · Posture {:?} · Gourd {:?} · skill {} · memories held {} · carried {}/{stored}",
                        s.index + 1,
                        sekiro::fmt_playtime(s.playtime_secs),
                        s.sen.map(sekiro::fmt_num).unwrap_or_default(),
                        s.attack,
                        s.vitality,
                        s.hp,
                        s.posture,
                        s.gourd,
                        s.skill_points,
                        s.memories_held,
                        s.carried,
                        stored = s.stored,
                    );
                    println!("    bosses: {}", s.bosses.join(", "));
                    let idols: usize = s.idol_areas.iter().map(|(_, l, _)| l).sum();
                    let total: usize = s.idol_areas.iter().map(|(_, _, t)| t).sum();
                    println!("    idols: {idols}/{total}");
                }
                return Ok(());
            }
        }
        if let Ok(slots) = elden::parse(&bytes) {
            println!("Elden Ring save:");
            for s in slots.iter().filter(|s| s.active) {
                println!(
                    "  {}: Lv {} · runes {:?} · hp {:?} · attrs {:?}",
                    s.name.clone().unwrap_or_default(),
                    s.level,
                    s.runes,
                    s.hp,
                    s.attrs
                );
            }
            return Ok(());
        }
        println!("BND4, contents not mapped.");
        return Ok(());
    }
    if let Ok(s) = hollow::parse(&bytes) {
        println!(
            "Hollow Knight: health {:?}/{:?} · geo {:?} · {} played · nail {:?} · v{}",
            s.health,
            s.max_health,
            s.geo,
            s.play_secs.map(hollow::fmt_playtime).unwrap_or_default(),
            s.nail,
            s.version.unwrap_or_default()
        );
        return Ok(());
    }
    if let Ok(w) = minecraft::parse(&bytes) {
        println!(
            "Minecraft '{}': {} · {} min played{}",
            w.name,
            w.mode,
            w.play_mins,
            if w.cheats { " · cheats on" } else { "" }
        );
        return Ok(());
    }
    if let Ok(s) = shady::parse(&bytes) {
        println!(
            "Shady: scene {} checkpoint {} · furthest scene {} · origami {}",
            s.scene, s.checkpoint, s.max_scene, s.origami
        );
        return Ok(());
    }
    if let Ok(info) = gvas::parse(&bytes) {
        println!("{} · {}", info.engine, info.class);
        for (n, t) in info.props.iter().take(20) {
            println!("  {n} ({t})");
        }
        if !info.labels.is_empty() {
            println!(
                "  mentions: {}",
                info.labels
                    .iter()
                    .take(15)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if info.partial && info.props.is_empty() && info.labels.is_empty() {
            println!("  (file ends unexpectedly — shown what could be read)");
        }
        return Ok(());
    }
    if let Some((fields, keys)) = json_preview(&bytes) {
        println!("JSON save ({} keys):", keys);
        for (k, v) in fields {
            println!("  {k}: {v}");
        }
        return Ok(());
    }
    println!("Unknown format — backed up as-is.");
    Ok(())
}

// ---------------- UI ----------------

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        self.poll_events();
        self.poll_jobs();
        ctx.request_repaint_after(Duration::from_secs(2));

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Brand block: vault-shield mark + wordmark.
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        logo_mark(ui, 34.0);
                        ui.heading(
                            egui::RichText::new("OMNEXA")
                                .size(22.0)
                                .strong()
                                .color(TEXT),
                        );
                    });
                    ui.label(
                        egui::RichText::new("U N I V E R S A L   D A T A   S A V E R")
                            .size(10.5)
                            .color(CYAN),
                    );
                });
                ui.add_space(18.0);
                // Live vault stats — the gaming-dashboard strip.
                let total = self.games.len();
                let with_saves = self.games.iter().filter(|g| !g.saves.is_empty()).count();
                let saves: usize = self.games.iter().map(|g| g.saves.len()).sum();
                header_stat(ui, &total.to_string(), "games");
                header_stat(ui, &with_saves.to_string(), "with saves");
                header_stat(ui, &saves.to_string(), "files");
                header_stat(
                    ui,
                    &fmt_bytes(backup::backup_total(&self.backup_root)),
                    "vaulted",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(if self.view == Nav::Settings {
                            "✕ Close"
                        } else {
                            "⚙ Settings"
                        })
                        .clicked()
                    {
                        self.view = if self.view == Nav::Settings {
                            Nav::Games
                        } else {
                            Nav::Settings
                        };
                        self.root_edit = self.backup_root.to_string_lossy().to_string();
                    }
                    if ui.button("⟳ Re-scan").clicked() && !self.scanning {
                        self.start_scan();
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").color(WEAK));
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search your library…")
                        .desired_width(240.0),
                );
                if self.scanning {
                    ui.spinner();
                    ui.label(egui::RichText::new("Scanning library…").weak());
                }
                if let Some(b) = self.busy.clone() {
                    ui.spinner();
                    ui.label(egui::RichText::new(b).color(ACCENT).size(13.0));
                }
            });
            // Filter chips with live counts.
            let counts = self.counts();
            ui.horizontal(|ui| {
                for (label, f) in [
                    (format!("▦ All ({})", counts.0), Filter::All),
                    (format!("🛡 Protected ({})", counts.1), Filter::Protected),
                    (
                        format!("⚠ Needs backup ({})", counts.2),
                        Filter::NeedsBackup,
                    ),
                    (format!("▶ Running ({})", counts.3), Filter::Running),
                ] {
                    let on = self.filter == f;
                    let b = egui::Button::new(
                        egui::RichText::new(label).size(12.5).strong().color(TEXT),
                    )
                    .fill(if on { ACCENT } else { CARD })
                    .stroke(egui::Stroke::new(1.0_f32, if on { ACCENT } else { BORDER }));
                    if ui.add(b).clicked() {
                        self.filter = f;
                    }
                }
            });
            ui.add_space(6.0);
        });

        // Left nav rail: Games / Backups / Settings / About + storage card.
        egui::SidePanel::left("nav")
            .resizable(false)
            .default_width(196.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                for (label, icon, v) in [
                    ("Games", "🎮", Nav::Games),
                    ("Backups", "🗂", Nav::Backups),
                    ("Settings", "⚙", Nav::Settings),
                    ("About", "ⓘ", Nav::About),
                ] {
                    let on = self.view == v;
                    let b = egui::Button::new(
                        egui::RichText::new(format!("{icon}  {label}"))
                            .size(14.0)
                            .strong()
                            .color(if on { CYAN } else { WEAK }),
                    )
                    .fill(if on { CARD_HOVER } else { PANEL })
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        if on {
                            ACCENT
                        } else {
                            egui::Color32::TRANSPARENT
                        },
                    ))
                    .corner_radius(10.0)
                    .min_size(egui::vec2(ui.available_width(), 40.0));
                    if ui.add(b).clicked() {
                        self.view = v;
                        if v == Nav::Settings {
                            self.root_edit = self.backup_root.to_string_lossy().to_string();
                        }
                    }
                    ui.add_space(2.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    self.show_storage_card(ui)
                });
            });

        if self.view != Nav::Games {
            // Full-page views hide the library rail.
            egui::CentralPanel::default().show(ctx, |ui| match self.view {
                Nav::Backups => self.show_all_backups(ctx, ui),
                Nav::Settings => self.show_settings_panel(ui),
                Nav::About => self.show_about(ui),
                Nav::Games => {}
            });
        } else {
            egui::SidePanel::left("games")
                .resizable(true)
                .default_width(348.0)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    // Skeleton rows while scanning: perceived performance.
                    if self.scanning && self.games.is_empty() {
                        for _ in 0..6 {
                            egui::Frame::group(ui.style())
                                .inner_margin(egui::Margin::symmetric(10, 10))
                                .corner_radius(12.0)
                                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                                .fill(CARD)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(egui::RichText::new("  …  ").weak());
                                });
                            ui.add_space(4.0);
                        }
                        return;
                    }
                    if !self.selected.is_empty() {
                        ui.horizontal(|ui| {
                            let n = self.selected.len();
                            let b = egui::Button::new(
                                egui::RichText::new(format!("Back up {n} selected"))
                                    .strong()
                                    .color(TEXT),
                            )
                            .fill(ACCENT)
                            .corner_radius(10.0);
                            if ui.add_enabled(self.busy.is_none(), b).clicked() {
                                self.do_backup_batch();
                            }
                            if ui.button("Clear").clicked() {
                                self.selected.clear();
                            }
                        });
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("game-list")
                        .show(ui, |ui| {
                            let q = self.search.to_ascii_lowercase();
                            let now = games::now_secs();
                            let running = self.running_cache.set.clone();
                            // Precompute protection cheaply per row via dir check.
                            let items: Vec<(
                                usize,
                                String,
                                String,
                                bool,
                                bool,
                                bool,
                                String,
                                Option<PathBuf>,
                            )> = self
                                .games
                                .iter()
                                .enumerate()
                                .filter(|(_, g)| {
                                    q.is_empty() || g.name.to_ascii_lowercase().contains(&q)
                                })
                                .filter(|(_, g)| match self.filter {
                                    Filter::All => true,
                                    Filter::Protected => self.protection(g),
                                    Filter::NeedsBackup => {
                                        !g.saves.is_empty() && !self.protection(g)
                                    }
                                    Filter::Running => {
                                        let names = g.exe_names();
                                        names
                                            .iter()
                                            .any(|n| running.contains(&n.to_ascii_lowercase()))
                                    }
                                })
                                .map(|(i, g)| {
                                    let protected = self.protection(g);
                                    let names = g.exe_names();
                                    let is_running = names
                                        .iter()
                                        .any(|n| running.contains(&n.to_ascii_lowercase()))
                                        && names.iter().any(|n| !n.is_empty());
                                    let mut sub = if g.saves.is_empty() {
                                        "Installed · no saves yet".to_string()
                                    } else if protected {
                                        format!("Protected · {}", g.progress)
                                    } else {
                                        format!("Not backed up · {}", g.progress)
                                    };
                                    if let Some(t) = g.last_played {
                                        sub.push_str(&format!(
                                            " · {}",
                                            games::fmt_relative(now, t)
                                        ));
                                    }
                                    (
                                        i,
                                        g.name.clone(),
                                        sub,
                                        self.sel == Some(i),
                                        protected,
                                        is_running,
                                        g.watch_key(),
                                        logos::find_cached(&self.backup_root, g),
                                    )
                                })
                                .collect();
                            if items.is_empty() && !self.scanning {
                                ui.label(
                                    egui::RichText::new("No games match this view.")
                                        .weak()
                                        .size(13.0),
                                );
                            }
                            let mut clicked: Option<usize> = None;
                            let mut toggles: Vec<(String, bool)> = Vec::new();
                            for (i, name, sub, active, protected, is_running, key, logo) in items {
                                let (hit, toggled) = game_card(
                                    ctx,
                                    &mut self.textures,
                                    ui,
                                    &name,
                                    &sub,
                                    active,
                                    protected,
                                    is_running,
                                    self.selected.contains(&key),
                                    logo,
                                );
                                if hit {
                                    clicked = Some(i);
                                }
                                if toggled {
                                    let add = !self.selected.contains(&key);
                                    toggles.push((key, add));
                                }
                            }
                            for (key, add) in toggles {
                                if add {
                                    self.selected.insert(key);
                                } else {
                                    self.selected.remove(&key);
                                }
                            }
                            if let Some(i) = clicked {
                                self.sel = Some(i);
                                self.refresh_backups();
                            }
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            match self.sel {
                Some(i) if i < self.games.len() => self.show_game(ctx, ui, i),
                _ => self.show_welcome(ui),
            }
        });

        // Restore confirm modal.
        if let Some(pick) = self.restore_pick.clone() {
            let mut close = false;
            let mut go = false;
            egui::Window::new("Restore this backup?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(
                        "I'll keep a safety copy of what's there now first. Nothing is lost either way.",
                    ).size(13.0));
                    if let Some(b) = self.backups.iter().find(|b| b.path == pick) {
                        ui.label(egui::RichText::new(format!(
                            "{} · {} files · {}",
                            b.when,
                            b.files,
                            games::fmt_bytes(b.bytes)
                        )).color(WEAK).size(13.0));
                    }
                    ui.label(egui::RichText::new(
                        "If Steam Cloud is on, pause it while testing or it may overwrite the restore.",
                    ).weak().size(12.0));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        let b = egui::Button::new(
                            egui::RichText::new("Restore").strong().color(TEXT),
                        ).fill(ACCENT);
                        if ui.add(b).clicked() {
                            go = true;
                        }
                    });
                });
            if close {
                self.restore_pick = None;
            }
            if go {
                if let Some(i) = self.sel {
                    self.do_restore(i, pick);
                }
            }
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💾").color(WEAK));
                if let Some(last) = self.toasts.last() {
                    ui.label(
                        egui::RichText::new(&last.text)
                            .color(if last.good { GOOD } else { ERROR })
                            .size(13.0),
                    );
                } else {
                    ui.label(egui::RichText::new(&self.status).color(WEAK).size(13.0));
                }
            });
        });
    }
}

impl App {
    /// (all, protected, needs-backup, running) counts for the filter tabs.
    fn counts(&self) -> (usize, usize, usize, usize) {
        let running = self.running_cache.set.clone();
        let mut prot = 0;
        let mut need = 0;
        let mut run = 0;
        for g in &self.games {
            let p = self.protection(g);
            if p {
                prot += 1;
            } else if !g.saves.is_empty() {
                need += 1;
            }
            let names = g.exe_names();
            if names
                .iter()
                .any(|n| running.contains(&n.to_ascii_lowercase()))
                && names.iter().any(|n| !n.is_empty())
            {
                run += 1;
            }
        }
        (self.games.len(), prot, need, run)
    }

    /// Honest storage card: app-managed backup + save bytes, nothing invented.
    fn show_storage_card(&mut self, ui: &mut egui::Ui) {
        let vaulted = backup::backup_total(&self.backup_root);
        let saves: u64 = self.games.iter().map(|g| g.total_bytes).sum();
        let frac = if vaulted + saves > 0 {
            vaulted as f32 / (vaulted + saves) as f32
        } else {
            0.0
        };
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(12, 10))
            .corner_radius(14.0)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .fill(CARD)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Storage").color(WEAK).size(12.0));
                ui.add_space(2.0);
                storage_ring(ui, frac, &fmt_bytes(vaulted));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Backups {} · Saves {}",
                        fmt_bytes(vaulted),
                        fmt_bytes(saves)
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
                ui.label(
                    egui::RichText::new("On this PC only")
                        .color(WEAK_DIM)
                        .size(11.0),
                );
            });
    }

    /// Every backup of every game, newest first — with restore + safe delete.
    fn show_all_backups(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading(
            egui::RichText::new("All backups")
                .size(24.0)
                .strong()
                .color(TEXT),
        );
        let vaulted = backup::backup_total(&self.backup_root);
        ui.label(
            egui::RichText::new(format!("{} protected on this PC", fmt_bytes(vaulted)))
                .color(WEAK)
                .size(13.0),
        );
        ui.add_space(8.0);
        let mut rows: Vec<(String, String, backup::BackupInfo)> = Vec::new();
        for g in &self.games {
            for b in backup::list_backup_infos(&self.backup_root, g) {
                rows.push((g.name.clone(), g.watch_key(), b));
            }
        }
        rows.sort_by(|a, b| b.2.path.cmp(&a.2.path));
        if rows.is_empty() {
            ui.label(
                egui::RichText::new("No backups yet. Pick a game and hit “Back up now”.")
                    .weak()
                    .size(13.0),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("all-backups")
            .show(ui, |ui| {
                let mut restore: Option<(usize, PathBuf)> = None;
                let mut delete: Option<PathBuf> = None;
                for (game, key, b) in &rows {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(12.0)
                        .stroke(egui::Stroke::new(1.0_f32, BORDER))
                        .fill(CARD)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(game).strong().size(14.5).color(TEXT),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {} files · {}{}",
                                            b.label,
                                            b.files,
                                            fmt_bytes(b.bytes),
                                            if b.verified {
                                                " · ✓ verified"
                                            } else {
                                                " · couldn't verify"
                                            }
                                        ))
                                        .color(if b.verified { GOOD } else { WARN })
                                        .size(12.5),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if self.delete_arm.as_ref() == Some(&b.path) {
                                            if ui
                                                .button(
                                                    egui::RichText::new("Confirm delete")
                                                        .color(ERROR),
                                                )
                                                .clicked()
                                            {
                                                delete = Some(b.path.clone());
                                            }
                                        } else if ui.button("Delete").clicked() {
                                            self.delete_arm = Some(b.path.clone());
                                        }
                                        if ui.button("Folder").clicked() {
                                            let _ = std::process::Command::new("explorer")
                                                .arg(&b.path)
                                                .spawn();
                                        }
                                        let idx =
                                            self.games.iter().position(|g| g.watch_key() == *key);
                                        let can = idx.is_some() && self.busy.is_none();
                                        if ui
                                            .add_enabled(can, egui::Button::new("Restore"))
                                            .clicked()
                                        {
                                            if let Some(i) = idx {
                                                restore = Some((i, b.path.clone()));
                                            }
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
                if let Some(p) = delete {
                    if std::fs::remove_dir_all(&p).is_ok() {
                        self.delete_arm = None;
                        self.refresh_backups();
                        self.toast("Backup deleted.", true);
                    } else {
                        self.toast("Couldn't delete that folder — it may be in use.", false);
                    }
                }
                drop(rows);
                if let Some((i, p)) = restore {
                    self.sel = Some(i);
                    self.refresh_backups();
                    self.restore_pick = Some(p);
                    self.view = Nav::Games;
                }
            });
        let _ = ctx;
    }

    /// About: what the app is, versions, credits.
    fn show_about(&self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            logo_mark(ui, 54.0);
            ui.vertical(|ui| {
                ui.heading(
                    egui::RichText::new("OMNEXA")
                        .size(26.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new("Universal Data Saver · your progress, safe.")
                        .color(WEAK)
                        .size(13.5),
                );
            });
        });
        ui.add_space(10.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(14, 10))
            .corner_radius(12.0)
            .stroke(egui::Stroke::new(1.0_f32, BORDER))
            .fill(CARD)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .size(13.5),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Backups live in {}",
                        self.backup_root.to_string_lossy()
                    ))
                    .color(WEAK)
                    .size(13.0),
                );
                ui.hyperlink_to(
                    "github.com/harshit51290/Omnexa",
                    "https://github.com/harshit51290/Omnexa",
                );
            });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Save-format research stands on community work: Sekiro/Elden Ring offsets follow darthdemono/sl2-analyzer (MIT); Hollow Knight crypto follows the community-published key; Unreal GVAS framing follows public engine knowledge. Cover art: Steam library headers, SteamGridDB, or each game's own icon. MIT licensed.",
            )
            .color(WEAK)
            .size(12.5),
        );
    }

    /// First-run welcome: the vault pitch, gaming style.
    fn show_welcome(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("welcome").show(ui, |ui| {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                logo_mark(ui, 54.0);
                ui.label(
                    egui::RichText::new("OMNEXA")
                        .size(15.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.heading(
                    egui::RichText::new("Your library, bulletproof.")
                        .size(34.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(
                        "Every save, every character, every world — found automatically, backed up in one click, restored without fear. Everything stays on this PC.",
                    )
                    .size(14.5)
                    .color(WEAK),
                );
            });
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                for (n, t, d) in [
                    ("01", "PICK A GAME", "Your real library floats to the top, cover art included."),
                    ("02", "BACK IT UP", "One click. Every file fingerprinted and verified."),
                    ("03", "AUTO-PROTECT", "Quit the game — a copy lands in your vault by itself."),
                ] {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .corner_radius(14.0)
                        .stroke(egui::Stroke::new(1.0_f32, BORDER))
                        .fill(CARD)
                        .show(ui, |ui| {
                            ui.set_width(210.0);
                            ui.label(
                                egui::RichText::new(n).strong().size(20.0).color(CYAN),
                            );
                            ui.label(
                                egui::RichText::new(t).strong().size(14.0).color(TEXT),
                            );
                            ui.label(egui::RichText::new(d).size(12.5).color(WEAK));
                        });
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("⟳ Re-scan").clicked() && !self.scanning {
                    self.start_scan();
                }
                if ui.button("⚙ Open Settings").clicked() {
                    self.view = Nav::Settings;
                    self.root_edit = self.backup_root.to_string_lossy().to_string();
                }
            });
            if self.scanning {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Looking for your games…").weak().size(13.0));
                });
            }
        });
    }
}
impl App {
    fn show_game(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, i: usize) {
        let g = self.games[i].clone();
        let key = g.watch_key();
        let logo = logos::find_cached(&self.backup_root, &g);
        let hist = db::history(&self.backup_root, &key);
        let watched = self
            .settings
            .watched
            .iter()
            .any(|w| w == &key || same_root(w, &key));
        let running = {
            let names = g.exe_names();
            let set = self.running_now();
            names.iter().any(|n| set.contains(&n.to_ascii_lowercase()))
                && names.iter().any(|n| !n.is_empty())
        };
        let twin_warn = backup::twin_status(&g);

        egui::ScrollArea::vertical()
            .id_salt(format!("detail-{i}"))
            .show(ui, |ui| {
                // Cinematic hero: full-width banner, big title, stat chips.
                if let Some(path) = logo.clone() {
                    if let Some(tex) = logo_texture(ctx, &mut self.textures, &path) {
                        egui::Frame::new()
                            .corner_radius(16.0)
                            .stroke(egui::Stroke::new(1.0_f32, BORDER))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(
                                    egui::Image::new(&tex)
                                        .fit_to_exact_size(egui::vec2(
                                            ui.available_width(),
                                            210.0,
                                        ))
                                        .corner_radius(14.0),
                                );
                            });
                        ui.add_space(8.0);
                    }
                }
                ui.horizontal(|ui| {
                    if logo.is_none() {
                        avatar(ui, &g.name, 56.0);
                    }
                    ui.vertical(|ui| {
                        if running {
                            ui.label(
                                egui::RichText::new("● RUNNING NOW")
                                    .size(11.5)
                                    .strong()
                                    .color(GOOD),
                            );
                        }
                        ui.heading(
                            egui::RichText::new(&g.name)
                                .size(30.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.label(
                            egui::RichText::new(&g.progress)
                                .size(15.0)
                                .color(CYAN),
                        );
                    });
                });
                ui.add_space(6.0);
                // Stat chips: last played · files · size · protection.
                ui.horizontal_wrapped(|ui| {
                    if let Some(t) = g.last_played {
                        detail_chip(
                            ui,
                            "🕒",
                            "LAST PLAYED",
                            &games::fmt_relative(games::now_secs(), t),
                        );
                    }
                    detail_chip(ui, "🗂", "SAVE FILES", &format!("{}", g.saves.len()));
                    detail_chip(ui, "💾", "SAVE SIZE", &fmt_bytes(g.total_bytes));
                    detail_chip(
                        ui,
                        "🛡",
                        "STATUS",
                        if self.backups.is_empty() {
                            "Needs backup"
                        } else {
                            "Protected"
                        },
                    );
                });
                ui.add_space(2.0);
                ui.label(backup_badge(!self.backups.is_empty(), &self.last_backup_line()));
                if running {
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(10.0)
                        .stroke(egui::Stroke::new(1.0_f32, WARN))
                        .fill(CARD)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(
                                "This game looks open right now. Quit it first — copying files mid-save can make a broken backup, and Restore stays locked until then.",
                            ).color(WARN).size(13.0));
                        });
                }
                if let Some(w) = twin_warn {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("⚠ {w}")).color(WARN).size(13.0));
                }
                ui.add_space(8.0);

                // Hero actions: big neon primary + outlined restore.
                let busy = self.busy.is_some();
                ui.horizontal_wrapped(|ui| {
                    let b = egui::Button::new(
                        egui::RichText::new(if busy { "WORKING…" } else { "▶  BACK UP NOW" })
                            .strong()
                            .color(TEXT)
                            .size(16.0),
                    )
                    .fill(ACCENT)
                    .corner_radius(12.0)
                    .min_size(egui::vec2(220.0, 52.0));
                    if ui.add_enabled(!g.saves.is_empty() && !busy, b).clicked() {
                        self.do_backup(i);
                    }
                    let why = if self.backups.is_empty() {
                        "No backups yet — back up first"
                    } else if running {
                        "Quit the game first"
                    } else if busy {
                        "Working…"
                    } else {
                        ""
                    };
                    let r = egui::Button::new(
                        egui::RichText::new("RESTORE").size(15.0).strong().color(TEXT),
                    )
                    .stroke(egui::Stroke::new(1.5_f32, CYAN))
                    .corner_radius(12.0)
                    .min_size(egui::vec2(170.0, 52.0));
                    let resp = ui.add_enabled(!self.backups.is_empty() && !running && !busy, r);
                    if resp.clicked() {
                        // Restore the newest by default; full list below for older ones.
                        self.restore_pick = Some(self.backups[0].path.clone());
                    }
                    if !why.is_empty() {
                        resp.on_hover_text(why);
                        ui.label(egui::RichText::new(why).weak().size(13.0));
                    }
                });
                ui.label(egui::RichText::new(
                    "Restore keeps a safety copy of what's there now first. If Steam Cloud is on, pause it while testing or it may overwrite the restore.",
                ).weak().size(13.0));
                ui.add_space(6.0);

                // Auto-protect toggle
                if !g.saves.is_empty() {
                    let mut w = watched;
                    if ui
                        .checkbox(&mut w, "Auto-protect: save a copy every time I quit this game")
                        .changed()
                    {
                        if w {
                            if !self.settings.watched.iter().any(|x| x == &key || same_root(x, &key)) {
                                self.settings.watched.push(key.clone());
                            }
                            if !self.settings.auto_backup {
                                self.settings.auto_backup = true;
                            }
                            self.toast(format!(
                                "Auto-protect on for {} — quit the game and I'll handle the rest.",
                                g.name
                            ), true);
                        } else {
                            self.settings.watched.retain(|x| !(x == &key || same_root(x, &key)));
                            self.toast(format!("Auto-protect off for {}.", g.name), true);
                        }
                        settings::save(&self.settings);
                    }
                }

                // Backup history: every entry restorable, verified badge, honest sizes.
                ui.add_space(8.0);
                section_title(
                    ui,
                    "🕒",
                    "BACKUP TIMELINE",
                    &format!("{} total", self.backups.len()),
                );
                if self.backups.is_empty() {
                    ui.label(egui::RichText::new(
                        "No backups yet. Hit “Back up now” — it takes seconds.",
                    ).weak().size(13.0));
                }
                for b in self.backups.clone() {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(10.0)
                        .stroke(egui::Stroke::new(1.0_f32, BORDER))
                        .fill(CARD)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&b.label).strong().size(14.0).color(TEXT));
                                    ui.label(egui::RichText::new(format!(
                                        "{} files · {}{}",
                                        b.files,
                                        games::fmt_bytes(b.bytes),
                                        if b.verified { " · ✓ verified" } else { " · couldn't verify" }
                                    )).color(if b.verified { GOOD } else { WARN }).size(13.0));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("Restore").clicked() && !running && self.busy.is_none() {
                                        self.restore_pick = Some(b.path.clone());
                                    }
                                    if ui.button("Folder").on_hover_text("Open this backup in Explorer").clicked() {
                                        let _ = std::process::Command::new("explorer")
                                            .arg(&b.path)
                                            .spawn();
                                    }
                                });
                            });
                        });
                    ui.add_space(4.0);
                }

                // What's inside the save
                match self.save_view(&g) {
                    SaveView::Sekiro(slots) => {
                        let used: Vec<&sekiro::SekiroSlot> =
                            slots.iter().filter(|s| s.used).collect();
                        if !used.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("◆ YOUR JOURNEY").color(CYAN).size(12.5).strong());
                            for s in used {
                                sekiro_card(ui, s);
                            }
                        }
                    }
                    SaveView::Er(slots) => {
                        let used: Vec<&elden::ErSlot> =
                            slots.iter().filter(|s| s.active).collect();
                        if !used.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("◆ YOUR CHARACTERS").color(CYAN).size(12.5).strong());
                            for s in used {
                                er_card(ui, s);
                            }
                        }
                    }
                    SaveView::Hollow(saves) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("◆ YOUR KNIGHT").color(CYAN).size(12.5).strong());
                        for (label, s) in &saves {
                            hollow_card(ui, label, s);
                        }
                    }
                    SaveView::Ue(agg) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("◆ CHECKPOINTS").color(CYAN).size(12.5).strong());
                        ue_card(ui, &agg);
                    }
                    SaveView::Mc(worlds) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("◆ YOUR WORLDS").color(CYAN).size(12.5).strong());
                        for (label, w) in &worlds {
                            mc_card(ui, label, w);
                        }
                    }
                    SaveView::Shady(s) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("◆ YOUR STORY").color(CYAN).size(12.5).strong());
                        shady_card(ui, &s);
                    }
                    SaveView::Json { file, fields, keys } => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("◆ INSIDE THE SAVE").color(CYAN).size(12.5).strong());
                        json_card(ui, &file, &fields, keys);
                    }
                    SaveView::Other => {
                        if g.fs.is_some() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(
                                "Couldn't read the details inside — backup still works normally.",
                            ).weak());
                        }
                    }
                }

                ui.add_space(6.0);
                ui.collapsing("Details", |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} files · {} · found via {}",
                            g.saves.len(),
                            fmt_bytes(g.total_bytes),
                            g.source
                        )).size(13.0),
                    );
                    for f in g.saves.iter().take(20) {
                        ui.label(
                            egui::RichText::new(
                                f.path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                            )
                            .monospace()
                            .size(13.0)
                            .color(WEAK),
                        );
                    }
                    if let Some(since) = hist.first_seen {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!(
                            "Tracked since {since} · {} backups",
                            hist.backups
                        )).color(WEAK).size(13.0));
                        for (when, what) in hist.history.iter().take(5) {
                            ui.label(egui::RichText::new(format!("  {when} — {what}")).color(WEAK).size(13.0));
                        }
                    }
                });
            });
    }

    fn show_settings_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("settings-scroll").show(ui, |ui| {
            ui.heading("Settings");
            ui.add_space(6.0);
            ui.label(egui::RichText::new("BACKUPS").color(CYAN).size(12.5).strong());
            ui.label(egui::RichText::new("Where backups live").strong().size(14.0).color(TEXT));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.root_edit);
                if ui.button("…").on_hover_text("Browse").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.root_edit = p.to_string_lossy().to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Use this folder").clicked() {
                    let p = PathBuf::from(self.root_edit.trim());
                    if p.is_absolute() {
                        self.backup_root = p.clone();
                        self.settings = settings::load(&p);
                        self.settings.backup_root = p;
                        settings::save(&self.settings);
                        self.refresh_backups();
                        self.toast("Backup folder updated.", true);
                    } else {
                        self.toast("That folder path doesn't look right.", false);
                    }
                }
                if ui.button("Default").clicked() {
                    self.backup_root = backup::default_backup_root();
                    self.root_edit = self.backup_root.to_string_lossy().to_string();
                    self.settings = settings::load(&self.backup_root);
                    self.refresh_backups();
                }
                if ui.button("📂 Reveal").on_hover_text("Open in Explorer").clicked() {
                    let _ = std::process::Command::new("explorer")
                        .arg(&self.backup_root)
                        .spawn();
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.label(egui::RichText::new("COVER ART").color(CYAN).size(12.5).strong());
            ui.label(
                egui::RichText::new(
                    "Steam art is found automatically. For the rest, paste a free SteamGridDB key (steamgriddb.com → your profile → API).",
                )
                .weak()
                .size(13.0),
            );
            let mut key = self.settings.sgdb_key.clone();
            if ui
                .text_edit_singleline(&mut key)
                .on_hover_text("SteamGridDB API key")
                .changed()
            {
                self.settings.sgdb_key = key;
                settings::save(&self.settings);
            }
            if ui.button("Fetch missing art now").clicked() {
                let root = self.backup_root.clone();
                let games = self.games.clone();
                let sgdb_key = self.settings.sgdb_key.clone();
                std::thread::spawn(move || {
                    logos::ensure_all(&root, &games, &sgdb_key);
                });
                self.toast("Fetching art in the background…", true);
            }
            ui.add_space(8.0);
            ui.separator();
            ui.label(egui::RichText::new("AUTO-PROTECT").color(CYAN).size(12.5).strong());
            let mut ab = self.settings.auto_backup;
            if ui.checkbox(&mut ab, "Auto-backup when I quit a watched game").changed() {
                self.settings.auto_backup = ab;
                settings::save(&self.settings);
                self.toast(if ab {
                    "Auto-backup on — tick games to watch them."
                } else {
                    "Auto-backup paused."
                }.to_string(), true);
            }
            ui.label(format!(
                "Watching {} game{} · keeping last {} backups each · {} stored",
                self.settings.watched.len(),
                if self.settings.watched.len() == 1 { "" } else { "s" },
                self.settings.retention,
                fmt_bytes(backup::backup_total(&self.backup_root)),
            ));
            ui.horizontal(|ui| {
                ui.label("Keep last");
                let mut r = self.settings.retention as i32;
                if ui.add(egui::DragValue::new(&mut r).range(1..=30)).changed() {
                    self.settings.retention = r as usize;
                    settings::save(&self.settings);
                }
                ui.label("backups per game");
            });
            if ui.button("Clear watched games").clicked() {
                self.settings.watched.clear();
                settings::save(&self.settings);
                self.toast("Watch list cleared.", true);
            }
            ui.add_space(8.0);
            ui.separator();
            ui.label(egui::RichText::new("GAME FOLDERS").color(CYAN).size(12.5).strong());
            ui.label(egui::RichText::new(
                "Portable games or launchers I missed? Point me at them.",
            ).weak().size(13.0));
            let mut remove: Option<usize> = None;
            for (k, c) in self.settings.custom_folders.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(c.to_string_lossy()).monospace().size(13.0));
                    if ui.button("✕").clicked() {
                        remove = Some(k);
                    }
                });
            }
            if let Some(k) = remove {
                self.settings.custom_folders.remove(k);
                settings::save(&self.settings);
                if !self.scanning {
                    self.start_scan();
                }
            }
            if ui.button("＋ Add folder…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    if !self.settings.custom_folders.contains(&p) {
                        self.settings.custom_folders.push(p);
                        settings::save(&self.settings);
                        if !self.scanning {
                            self.start_scan();
                        }
                        self.toast("Folder added — scanning again.", true);
                    }
                }
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(
                "How it works: leave Omnexa running while you play. When a watched game closes, a copy lands in your backup folder — nothing uploaded anywhere.",
            ).weak().size(13.0));
        });
    }
}
