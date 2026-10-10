mod backup;
mod bnd4;
mod db;
mod elden;
mod games;
mod gvas;
mod hollow;
mod logos;
mod minecraft;
mod sekiro;
mod settings;
mod shady;

use eframe::egui;
use games::{fmt_bytes, FsKind, GameEntry};
use settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// ---------------- "Ember" theme ----------------
// Warm dark: near-black blue surfaces, one amber accent, calm greens.

const BG: egui::Color32 = egui::Color32::from_rgb(11, 14, 20);
const PANEL: egui::Color32 = egui::Color32::from_rgb(16, 21, 30);
const CARD: egui::Color32 = egui::Color32::from_rgb(22, 29, 42);
const CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(28, 36, 52);
const BORDER: egui::Color32 = egui::Color32::from_rgb(38, 48, 73);
const TEXT: egui::Color32 = egui::Color32::from_rgb(232, 237, 245);
const WEAK: egui::Color32 = egui::Color32::from_rgb(142, 153, 173);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(245, 165, 36);
const ACCENT_INK: egui::Color32 = egui::Color32::from_rgb(20, 12, 2);
const GOOD: egui::Color32 = egui::Color32::from_rgb(52, 211, 153);
const WARN: egui::Color32 = egui::Color32::from_rgb(251, 191, 36);

const AVATAR_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(245, 165, 36),
    egui::Color32::from_rgb(96, 165, 250),
    egui::Color32::from_rgb(52, 211, 153),
    egui::Color32::from_rgb(192, 132, 252),
    egui::Color32::from_rgb(248, 113, 113),
    egui::Color32::from_rgb(45, 212, 191),
    egui::Color32::from_rgb(251, 146, 60),
    egui::Color32::from_rgb(148, 163, 184),
];

fn avatar_color(name: &str) -> egui::Color32 {
    let h: usize = name.bytes().fold(0, |a, b| a.wrapping_add(b as usize));
    AVATAR_COLORS[h % AVATAR_COLORS.len()]
}

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

#[derive(Clone, Default)]
struct UeAggregate {
    engine: String,
    class: String,
    files: usize,
    recent: Vec<(String, String)>,
    partial: bool,
    labels: Vec<String>,
}

enum ScanMsg {
    Done(Vec<GameEntry>),
}

struct App {
    games: Vec<GameEntry>,
    scanning: bool,
    scan_rx: mpsc::Receiver<ScanMsg>,
    scan_tx: mpsc::Sender<ScanMsg>,
    sel: Option<usize>,
    backup_root: PathBuf,
    settings: Settings,
    status: String,
    search: String,
    show_settings: bool,
    root_edit: String,
    events_rx: mpsc::Receiver<String>,
    backups: Vec<PathBuf>,
    views: HashMap<String, SaveView>,
    textures: HashMap<String, egui::TextureHandle>,
    running_cache: HashSetCache,
}

#[derive(Default)]
struct HashSetCache {
    set: std::collections::HashSet<String>,
    at: Option<Instant>,
}

impl App {
    fn new(events_rx: mpsc::Receiver<String>) -> Self {
        let backup_root = backup::default_backup_root();
        let settings = settings::load(&backup_root);
        let backup_root = settings.backup_root.clone();
        let (scan_tx, scan_rx) = mpsc::channel();
        let mut app = Self {
            games: Vec::new(),
            scanning: true,
            scan_rx,
            scan_tx,
            sel: None,
            backup_root,
            settings,
            status: "Looking for your games…".into(),
            search: String::new(),
            show_settings: false,
            root_edit: String::new(),
            events_rx,
            backups: Vec::new(),
            views: HashMap::new(),
            textures: HashMap::new(),
            running_cache: HashSetCache::default(),
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
            self.say(if n == 0 {
                "No saves found — add a folder in Settings if a game is missing.".to_string()
            } else {
                format!("Found {n} save files. Pick a game to protect it.")
            });
        }
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    fn refresh_backups(&mut self) {
        self.backups.clear();
        if let Some(i) = self.sel {
            if let Some(g) = self.games.get(i) {
                self.backups = backup::list_backups_for(&self.backup_root, &backup::dir_name(g));
            }
        }
    }

    fn last_backup_line(&self) -> String {
        match self.backups.first() {
            Some(p) => format!(
                "Last backup {}",
                p.file_name().and_then(|s| s.to_str()).unwrap_or("recently")
            ),
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
                        self.say(format!(
                            "Auto backup done for {} — you're safe.",
                            self.games[i].name
                        ));
                        if self.sel == Some(i) {
                            self.refresh_backups();
                        }
                    }
                    Err(e) => self.say(backup::friendly_err(&e)),
                }
            }
        }
    }

    fn do_backup(&mut self, i: usize) {
        let mut g = self.games[i].clone();
        backup::refresh_saves(&mut g);
        self.games[i] = g.clone();
        let key = g.watch_key();
        let (nfiles, nbytes) = (g.saves.len(), g.total_bytes);
        match backup::backup_game(&g, &self.backup_root, self.settings.retention) {
            Ok(dest) => {
                db::record_backup(&self.backup_root, &key, &dest, "manual", nfiles, nbytes);
                self.views.clear();
                self.say(format!("Backed up {} — relax, it's safe.", g.name));
                self.refresh_backups();
            }
            Err(e) => self.say(backup::friendly_err(&e)),
        }
    }

    fn do_restore(&mut self, i: usize) {
        if self.backups.is_empty() {
            return;
        }
        // Safety first: snapshot whatever is there now.
        let mut g = self.games[i].clone();
        backup::refresh_saves(&mut g);
        if let Ok(snap) = backup::snapshot(&g, &self.backup_root) {
            db::record_backup(
                &self.backup_root,
                &g.watch_key(),
                &snap,
                "snapshot",
                g.saves.len(),
                g.total_bytes,
            );
        }
        let latest = self.backups[0].clone();
        match backup::restore_backup(&latest) {
            Ok((n, _)) => {
                self.views.clear();
                backup::refresh_saves(&mut g);
                self.games[i] = g;
                self.say(format!(
                    "Restored {n} files. I kept a copy of what was there, just in case."
                ));
                self.refresh_backups();
            }
            Err(e) => self.say(backup::friendly_err(&e)),
        }
    }
}

fn same_root(a: &str, b: &str) -> bool {
    let ra = a.split('|').next().unwrap_or("");
    let rb = b.split('|').next().unwrap_or("");
    !ra.is_empty() && ra.eq_ignore_ascii_case(rb)
}

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--scan" || a == "--details" || a == "--selftest") {
        return headless();
    }
    let (tx, rx) = mpsc::channel::<String>();
    let boot_root = backup::default_backup_root();
    let boot_root = settings::load(&boot_root).backup_root;
    settings::spawn_watchdog(boot_root, tx);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title("SaveVault — game save backup"),
        ..Default::default()
    };
    eframe::run_native(
        "savevault",
        opts,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.window_fill = BG;
            style.visuals.panel_fill = PANEL;
            style.visuals.faint_bg_color = CARD;
            style.visuals.extreme_bg_color = BG;
            style.visuals.widgets.inactive.bg_fill = CARD;
            style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
            style.visuals.widgets.hovered.bg_fill = CARD_HOVER;
            style.visuals.widgets.active.bg_fill = CARD_HOVER;
            style.visuals.selection.bg_fill = ACCENT;
            style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT_INK);
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(16.0, 10.0);
            cc.egui_ctx.set_style(style);
            cc.egui_ctx.set_pixels_per_point(1.15);
            Ok(Box::new(App::new(rx)))
        }),
    )
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

fn headless() -> eframe::Result<()> {
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
        println!("usage: savevault --details <save file>");
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
        ctx.request_repaint_after(Duration::from_secs(2));

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("SaveVault")
                        .size(24.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("your progress, safe").weak().size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(if self.show_settings {
                            "✕ Close"
                        } else {
                            "⚙ Settings"
                        })
                        .clicked()
                    {
                        self.show_settings = !self.show_settings;
                        self.root_edit = self.backup_root.to_string_lossy().to_string();
                    }
                    if ui.button("⟳ Re-scan").clicked() && !self.scanning {
                        self.start_scan();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").color(WEAK));
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Find a game…")
                        .desired_width(280.0),
                );
                if self.scanning {
                    ui.spinner();
                    ui.label(egui::RichText::new("Scanning…").weak());
                }
            });
            ui.add_space(6.0);
        });

        if self.show_settings {
            egui::SidePanel::right("settings")
                .resizable(false)
                .default_width(320.0)
                .show(ctx, |ui| self.show_settings_panel(ui));
        }

        egui::SidePanel::left("games")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("game-list")
                    .show(ui, |ui| {
                        let q = self.search.to_ascii_lowercase();
                        let now = games::now_secs();
                        let items: Vec<(
                            usize,
                            String,
                            String,
                            bool,
                            bool,
                            Option<PathBuf>,
                            String,
                        )> = self
                            .games
                            .iter()
                            .enumerate()
                            .filter(|(_, g)| {
                                q.is_empty() || g.name.to_ascii_lowercase().contains(&q)
                            })
                            .map(|(i, g)| {
                                let mut sub = if g.saves.is_empty() {
                                    "Installed · no saves".to_string()
                                } else {
                                    g.progress.clone()
                                };
                                if let Some(t) = g.last_played {
                                    sub.push_str(&format!(" · {}", games::fmt_relative(now, t)));
                                }
                                (
                                    i,
                                    g.name.clone(),
                                    sub,
                                    self.sel == Some(i),
                                    !g.saves.is_empty(),
                                    logos::find_cached(&self.backup_root, g),
                                    g.source.to_string(),
                                )
                            })
                            .collect();
                        if items.is_empty() && !self.scanning {
                            ui.label(egui::RichText::new("No games match.").weak());
                        }
                        let mut clicked: Option<usize> = None;
                        for (i, name, sub, active, has, logo, source) in items {
                            if game_card(
                                ctx,
                                &mut self.textures,
                                ui,
                                &name,
                                &sub,
                                &source,
                                active,
                                has,
                                logo,
                            ) {
                                clicked = Some(i);
                            }
                        }
                        if let Some(i) = clicked {
                            self.sel = Some(i);
                            self.refresh_backups();
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            match self.sel {
                Some(i) if i < self.games.len() => self.show_game(ctx, ui, i),
                _ => {
                    ui.centered_and_justified(|ui| {
                        ui.label(if self.scanning {
                            "Looking for your games…"
                        } else {
                            "Pick a game on the left to protect it."
                        });
                    });
                }
            }
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💾").color(WEAK));
                ui.label(&self.status);
            });
        });
    }
}

/// One friendly game row: cover thumb (or initial bauble), name, status.
/// Returns true when clicked.
fn game_card(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    ui: &mut egui::Ui,
    name: &str,
    sub: &str,
    source: &str,
    active: bool,
    has: bool,
    logo: Option<PathBuf>,
) -> bool {
    let frame = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(12.0)
        .stroke(if active {
            egui::Stroke::new(1.5_f32, ACCENT)
        } else {
            egui::Stroke::new(1.0_f32, BORDER)
        })
        .fill(if active { CARD_HOVER } else { CARD });
    let mut clicked = false;
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let r = ui.horizontal(|ui| {
            if let Some(tex) = logo.as_ref().and_then(|p| logo_texture(ctx, textures, p)) {
                ui.add(
                    egui::Image::new(&tex)
                        .fit_to_exact_size(egui::vec2(64.0, 38.0))
                        .corner_radius(6.0),
                );
            } else {
                avatar(ui, name, 38.0);
            }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(name).strong().size(14.0).color(TEXT));
                ui.label(egui::RichText::new(sub).color(WEAK).size(12.0));
                ui.label(egui::RichText::new(source).color(WEAK).size(10.0));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("●")
                        .color(if has { GOOD } else { WEAK })
                        .size(12.0),
                );
            });
        });
        if r.response.interact(egui::Sense::click()).clicked() {
            clicked = true;
        }
    });
    ui.add_space(4.0);
    clicked
}

/// GPU texture for a cached logo file, loaded once.
fn logo_texture(
    ctx: &egui::Context,
    textures: &mut HashMap<String, egui::TextureHandle>,
    path: &PathBuf,
) -> Option<egui::TextureHandle> {
    let key = path.to_string_lossy().to_string();
    if let Some(h) = textures.get(&key) {
        return Some(h.clone());
    }
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || w * h > 8_000_000 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([w, h], &img.into_raw());
    let handle = ctx.load_texture(&key, color, egui::TextureOptions::LINEAR);
    textures.insert(key, handle.clone());
    Some(handle)
}

/// Colored initial bauble: games are recognisable at a glance, no icons needed.
fn avatar(ui: &mut egui::Ui, name: &str, size: f32) {
    let initial = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('?')
        .to_string()
        .to_uppercase();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, avatar_color(name));
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(size * 0.45),
        ACCENT_INK,
    );
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
                // Hero banner, then identity
                if let Some(path) = logo.clone() {
                    if let Some(tex) = logo_texture(ctx, &mut self.textures, &path) {
                        ui.add(egui::Image::new(&tex).max_height(120.0));
                        ui.add_space(4.0);
                    }
                }
                ui.horizontal(|ui| {
                    if logo.is_none() {
                        avatar(ui, &g.name, 52.0);
                    }
                    ui.vertical(|ui| {
                        ui.heading(egui::RichText::new(&g.name).size(24.0).strong().color(TEXT));
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("via {}", g.source))
                                    .color(WEAK)
                                    .size(12.0),
                            );
                            ui.label(egui::RichText::new(format!("· {}", g.progress)).size(13.0));
                            if let Some(t) = g.last_played {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "· last played {}",
                                        games::fmt_relative(games::now_secs(), t)
                                    ))
                                    .color(WEAK)
                                    .size(12.0),
                                );
                            }
                        });
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(backup_badge(!self.backups.is_empty(), &self.last_backup_line()));
                });
                if running {
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(10.0)
                        .stroke(egui::Stroke::new(1.0_f32, WARN))
                        .fill(CARD)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(
                                "⚠ This game looks open right now. Quit it first — copying files mid-save can make a broken backup, and Restore is locked until then.",
                            ).color(WARN));
                        });
                }
                if let Some(w) = twin_warn {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("⚠ {w}")).color(WARN));
                }
                ui.add_space(8.0);

                // Hero actions
                ui.horizontal(|ui| {
                    let big = egui::vec2(210.0, 52.0);
                    let can = !g.saves.is_empty();
                    let b = egui::Button::new(
                        egui::RichText::new("⬇  Back up now").strong().color(ACCENT_INK).size(15.0),
                    )
                    .min_size(big)
                    .fill(ACCENT);
                    if ui.add_enabled(can, b).clicked() {
                        self.do_backup(i);
                    }
                    let r = egui::Button::new("↩  Restore").min_size(big);
                    let why = if self.backups.is_empty() {
                        "No backups yet"
                    } else if running {
                        "Quit the game first"
                    } else {
                        ""
                    };
                    let resp = ui.add_enabled(!self.backups.is_empty() && !running, r);
                    if resp.clicked() {
                        self.do_restore(i);
                    }
                    if !why.is_empty() {
                        resp.on_hover_text(why);
                        ui.label(egui::RichText::new(why).weak().size(12.0));
                    }
                });
                ui.label(egui::RichText::new(
                    "Restore puts the newest backup back and keeps a copy of what was there, just in case. If Steam Cloud is on, pause it while testing or it may overwrite the restore.",
                ).weak().size(12.0));
                ui.add_space(6.0);

                // Auto-protect toggle
                if !g.saves.is_empty() {
                    let mut w = watched;
                    if ui
                        .checkbox(&mut w, "Auto-backup: save a copy every time I quit this game")
                        .changed()
                    {
                        if w {
                            if !self.settings.watched.iter().any(|x| x == &key || same_root(x, &key)) {
                                self.settings.watched.push(key.clone());
                            }
                            if !self.settings.auto_backup {
                                self.settings.auto_backup = true;
                            }
                            self.say(format!(
                                "Auto-backup on for {} — quit the game and I'll handle the rest.",
                                g.name
                            ));
                        } else {
                            self.settings.watched.retain(|x| !(x == &key || same_root(x, &key)));
                            self.say(format!("Auto-backup off for {}.", g.name));
                        }
                        settings::save(&self.settings);
                    }
                }

                // What's inside the save
                match self.save_view(&g) {
                    SaveView::Sekiro(slots) => {
                        let used: Vec<&sekiro::SekiroSlot> =
                            slots.iter().filter(|s| s.used).collect();
                        if !used.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("YOUR JOURNEY").color(WEAK).size(11.0));
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
                            ui.label(egui::RichText::new("YOUR CHARACTERS").color(WEAK).size(11.0));
                            for s in used {
                                er_card(ui, s);
                            }
                        }
                    }
                    SaveView::Hollow(saves) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("YOUR KNIGHT").color(WEAK).size(11.0));
                        for (label, s) in &saves {
                            hollow_card(ui, label, s);
                        }
                    }
                    SaveView::Ue(agg) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("CHECKPOINTS").color(WEAK).size(11.0));
                        ue_card(ui, &agg);
                    }
                    SaveView::Mc(worlds) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("YOUR WORLDS").color(WEAK).size(11.0));
                        for (label, w) in &worlds {
                            mc_card(ui, label, w);
                        }
                    }
                    SaveView::Shady(s) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("YOUR STORY").color(WEAK).size(11.0));
                        shady_card(ui, &s);
                    }
                    SaveView::Json { file, fields, keys } => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("INSIDE THE SAVE").color(WEAK).size(11.0));
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
                    ui.label(format!("{} files · {}", g.saves.len(), fmt_bytes(g.total_bytes)));
                    for f in g.saves.iter().take(20) {
                        ui.label(
                            egui::RichText::new(
                                f.path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                            )
                            .monospace()
                            .color(WEAK),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Recent backups").color(WEAK));
                    if self.backups.is_empty() {
                        ui.label("None yet.");
                    }
                    for b in &self.backups {
                        ui.label(
                            egui::RichText::new(
                                b.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                            )
                            .monospace()
                            .color(WEAK),
                        );
                    }
                    if let Some(since) = hist.first_seen {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!(
                            "Tracked since {since} · {} backups",
                            hist.backups
                        )).color(WEAK));
                        for (when, what) in hist.history.iter().take(5) {
                            ui.label(egui::RichText::new(format!("  {when} — {what}")).color(WEAK).size(12.0));
                        }
                    }
                });
            });
    }

    fn show_settings_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("settings-scroll").show(ui, |ui| {
            ui.heading("Settings");
            ui.add_space(6.0);
            ui.label("Where backups live");
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
                        self.say("Backup folder updated.");
                    } else {
                        self.say("That folder path doesn't look right.");
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
            ui.label("Cover art");
            ui.label(
                egui::RichText::new(
                    "Steam art is found automatically. For the rest, paste a free SteamGridDB key (steamgriddb.com → your profile → API).",
                )
                .weak()
                .size(12.0),
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
                self.say("Fetching art in the background…");
            }
            ui.add_space(8.0);
            ui.separator();
            let mut ab = self.settings.auto_backup;
            if ui.checkbox(&mut ab, "Auto-backup when I quit a watched game").changed() {
                self.settings.auto_backup = ab;
                settings::save(&self.settings);
                self.say(if ab {
                    "Auto-backup on — tick games to watch them."
                } else {
                    "Auto-backup paused."
                });
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
                self.say("Watch list cleared.");
            }
            ui.add_space(8.0);
            ui.separator();
            ui.label("Extra game folders");
            ui.label(egui::RichText::new(
                "Portable games or launchers I missed? Point me at them.",
            ).weak().size(12.0));
            let mut remove: Option<usize> = None;
            for (k, c) in self.settings.custom_folders.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(c.to_string_lossy()).monospace().size(12.0));
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
                        self.say("Folder added — scanning again.");
                    }
                }
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(
                "How it works: leave SaveVault running while you play. When a watched game closes, a copy lands in your backup folder — nothing uploaded anywhere.",
            ).weak().size(12.0));
        });
    }
}

/// One Sekiro character, explained like the game explains it.
fn sekiro_card(ui: &mut egui::Ui, s: &sekiro::SekiroSlot) {
    let journey = if s.journey == 0 {
        "First journey".to_string()
    } else {
        format!("Journey {} (NG+{})", s.journey + 1, s.journey)
    };
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Character {}", s.index + 1))
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.label(
                egui::RichText::new(format!(
                    "{journey} · {} played",
                    sekiro::fmt_playtime(s.playtime_secs)
                ))
                .color(WEAK),
            );
            ui.add_space(4.0);
            let num = sekiro::fmt_num;
            match s.attack {
                Some(a) if a > 1 => ui.label(format!("⚔ Attack {a} · {} bosses beaten", a - 1)),
                _ => ui.label("⚔ Attack 1 · no bosses beaten yet"),
            };
            match s.vitality {
                Some(v) if v > 1 => ui.label(format!("❤ Vitality {v} · {} necklaces used", v - 1)),
                _ => ui.label("❤ Vitality 1 · no necklaces yet"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Health {}",
                    s.hp.map(num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "Posture {}",
                    s.posture.map(num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("Sen {}", s.sen.map(num).as_deref().unwrap_or("—")));
            });
            match s.gourd {
                Some(g) if g > 1 => ui.label(format!(
                    "🎃 Gourd: {g} sips · {} seeds given to Emma",
                    g - 1
                )),
                _ => ui.label("🎃 Gourd: 1 sip"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("✦ {} skill points", num(s.skill_points)));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("💀 {} memories held", s.memories_held));
            });
            if !s.bosses.is_empty() {
                ui.label(
                    egui::RichText::new(format!("👹 Beaten: {}", s.bosses.join(" · "))).size(12.0),
                );
            }
            let idols_lit: usize = s.idol_areas.iter().map(|(_, l, _)| l).sum();
            let idols_total: usize = s.idol_areas.iter().map(|(_, _, t)| t).sum();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("🕯 {idols_lit} of {idols_total} idols lit"));
                if !s.endings.is_empty() {
                    ui.label(egui::RichText::new("·").color(WEAK));
                    ui.label(format!("🏁 {}", s.endings.join(", ")));
                }
            });
            if !s.cats.is_empty() {
                let line: Vec<String> = s.cats.iter().map(|(n, c)| format!("{n}: {c}")).collect();
                ui.label(
                    egui::RichText::new(format!(
                        "🎒 {} carried · {} in storage — {}",
                        s.carried,
                        s.stored,
                        line.join(" · ")
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
            let areas: Vec<String> = s
                .idol_areas
                .iter()
                .filter(|(_, l, _)| *l > 0)
                .map(|(a, l, t)| format!("{a} {l}/{t}"))
                .collect();
            if !areas.is_empty() {
                ui.label(
                    egui::RichText::new(areas.join(" · "))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

/// One Elden Ring character, explained like the status screen.
fn er_card(ui: &mut egui::Ui, s: &elden::ErSlot) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        s.name
                            .clone()
                            .unwrap_or_else(|| format!("Character {}", s.index + 1)),
                    )
                    .strong()
                    .size(15.0)
                    .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(format!("Level {}", s.level))
                        .color(GOOD)
                        .strong(),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "Runes {}",
                    s.runes.map(sekiro::fmt_num).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "Health {}",
                    s.hp.map(sekiro::fmt_num).as_deref().unwrap_or("—")
                ));
            });
            if let Some(a) = s.attrs {
                let names = elden::attr_names();
                let line: Vec<String> = names
                    .iter()
                    .zip(a.iter())
                    .map(|(n, v)| format!("{n} {v}"))
                    .collect();
                ui.label(egui::RichText::new(line.join(" · ")).color(WEAK).size(12.0));
            } else {
                ui.label(
                    egui::RichText::new("Details still being mapped for this one.").color(WEAK),
                );
            }
        });
    ui.add_space(4.0);
}

fn hollow_card(ui: &mut egui::Ui, label: &str, s: &hollow::HollowSave) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).strong().size(15.0).color(TEXT));
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "❤ {} masks",
                    s.health
                        .or(s.max_health)
                        .map(|n| n.to_string())
                        .as_deref()
                        .unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "🪙 {} geo",
                    s.geo.map(|n| n.to_string()).as_deref().unwrap_or("—")
                ));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!(
                    "🗡 Nail {}",
                    s.nail.map(|n| n.to_string()).as_deref().unwrap_or("—")
                ));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{} played",
                    s.play_secs
                        .map(hollow::fmt_playtime)
                        .unwrap_or_else(|| "—".into())
                ));
                if let Some(c) = s.completion {
                    if c > 0.0 {
                        ui.label(egui::RichText::new("·").color(WEAK));
                        ui.label(format!("{c:.0}% complete"));
                    }
                }
            });
            if let Some(v) = &s.version {
                ui.label(
                    egui::RichText::new(format!("game version {v}"))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

fn ue_card(ui: &mut egui::Ui, agg: &UeAggregate) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !agg.engine.is_empty() {
                ui.label(
                    egui::RichText::new(&agg.engine)
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
            }
            if !agg.class.is_empty() {
                ui.label(egui::RichText::new(&agg.class).color(WEAK).size(12.0));
            }
            ui.label(format!(
                "🗂 {} checkpoint file{}",
                agg.files,
                if agg.files == 1 { "" } else { "s" }
            ));
            for (name, when) in agg.recent.iter().take(5) {
                ui.label(
                    egui::RichText::new(format!(
                        "  • {name}{}",
                        if when.is_empty() {
                            String::new()
                        } else {
                            format!(" — {when}")
                        }
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
            if agg.partial && agg.labels.is_empty() {
                ui.label(
                    egui::RichText::new("Newest file ends mid-write — shown what could be read.")
                        .color(WEAK)
                        .size(12.0),
                );
            }
            if !agg.labels.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "Values mentioned: {}",
                        agg.labels
                            .iter()
                            .take(12)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ))
                    .color(WEAK)
                    .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

fn mc_card(ui: &mut egui::Ui, label: &str, w: &minecraft::McWorld) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if label.is_empty() { &w.name } else { label })
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} mode", w.mode));
                ui.label(egui::RichText::new("·").color(WEAK));
                ui.label(format!("{} played", hollow::fmt_playtime(w.play_mins * 60)));
                if w.cheats {
                    ui.label(egui::RichText::new("·").color(WEAK));
                    ui.label("cheats on");
                }
            });
        });
    ui.add_space(4.0);
}

fn shady_card(ui: &mut egui::Ui, s: &shady::ShadySave) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Scene {}", s.scene))
                        .strong()
                        .size(15.0)
                        .color(TEXT),
                );
                ui.label(egui::RichText::new("● In progress").color(GOOD).strong());
            });
            if !s.checkpoint.is_empty() {
                ui.label(format!("📍 Checkpoint: {}", s.checkpoint));
            }
            if s.max_scene > s.scene {
                ui.label(
                    egui::RichText::new(format!(
                        "Furthest: scene {}{}",
                        s.max_scene,
                        if s.max_checkpoint.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", s.max_checkpoint)
                        }
                    ))
                    .color(WEAK),
                );
            }
            ui.label(format!("🦢 Origami folded: {}", s.origami));
        });
    ui.add_space(4.0);
}

fn json_card(ui: &mut egui::Ui, file: &str, fields: &[(String, String)], keys: usize) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(12.0)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .fill(CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(file).strong().size(14.0).color(TEXT));
            for (k, v) in fields.iter().take(8) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{k}:")).color(WEAK).size(12.0));
                    ui.label(egui::RichText::new(v).size(12.0));
                });
            }
            if keys > fields.len() {
                ui.label(
                    egui::RichText::new(format!("+ {} more details", keys - fields.len()))
                        .color(WEAK)
                        .size(12.0),
                );
            }
        });
    ui.add_space(4.0);
}

fn backup_badge(has: bool, line: &str) -> egui::RichText {
    if has {
        egui::RichText::new(format!("✓ Protected · {line}"))
            .color(GOOD)
            .strong()
    } else {
        egui::RichText::new(format!("○ {line}")).color(WARN)
    }
}
