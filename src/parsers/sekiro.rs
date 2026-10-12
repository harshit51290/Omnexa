//! What's inside a Sekiro save slot, in plain numbers.
//!
//! Field positions verified against darthdemono/sl2-analyzer (MIT) and checked
//! against a real S0000.sl2 on this machine. All offsets are into the slot
//! payload — the entry data *after* its 16-byte MD5.

use anyhow::Result;

fn u32(b: &[u8], off: usize) -> Option<u32> {
    if off + 4 > b.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
    ]))
}

fn u8(b: &[u8], off: usize) -> Option<u8> {
    b.get(off).copied()
}

fn u64(b: &[u8], off: usize) -> Option<u64> {
    if off + 8 > b.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ]))
}

fn ranged(v: Option<u32>, lo: u32, hi: u32) -> Option<u32> {
    match v {
        Some(x) if lo <= x && x <= hi => Some(x),
        _ => None,
    }
}

/// A value the game stores twice: trusted only when both copies agree.
fn twin(b: &[u8], a: usize, c: usize) -> Option<u32> {
    match (u32(b, a), u32(b, c)) {
        (Some(x), Some(y)) if x != 0 && x == y => Some(x),
        _ => None,
    }
}

#[derive(Clone)]
pub struct SekiroSlot {
    pub index: usize,
    pub used: bool,
    pub playtime_secs: u32,
    pub journey: u8,
    pub sen: Option<u32>,
    pub attack: Option<u32>,
    pub vitality: Option<u32>,
    pub hp: Option<u32>,
    pub posture: Option<u32>,
    pub gourd: Option<u8>,
    pub skill_points: u32,
    pub memories_held: u32,
    pub carried: usize,
    pub stored: usize,
    /// (label, count) in display order, empties omitted.
    pub cats: Vec<(String, usize)>,
    pub bosses: Vec<String>,
    pub endings: Vec<String>,
    /// (area, lit, total)
    pub idol_areas: Vec<(String, usize, usize)>,
}

pub fn fmt_playtime(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}

pub fn fmt_num(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

/// Parse every character slot (entries 0-9) of an S0000.sl2 file.
pub fn parse(file: &[u8]) -> Result<Vec<SekiroSlot>> {
    let entries = crate::parsers::bnd4::entries(file)?;
    // Occupancy array lives in the menu entry (10), one byte per slot.
    // Falls back to content sniffing when unreadable — never hides characters.
    let occupancy: Option<Vec<bool>> = entries.get(10).and_then(|menu| {
        let base = menu.data_off + 16 + 212;
        let mut v = Vec::new();
        for i in 0..10 {
            let byte = *file.get(base + i)?;
            if byte > 1 {
                return None;
            }
            v.push(byte == 1);
        }
        if v.iter().any(|b| *b) {
            Some(v)
        } else {
            None
        }
    });

    let mut out = Vec::new();
    for (i, e) in entries.iter().take(10).enumerate() {
        let end = entries.get(i + 1).map(|n| n.data_off).unwrap_or(file.len());
        let payload = match file.get(e.data_off + 16..end) {
            Some(p) => p,
            None => continue,
        };
        let steam = u64(payload, 0x33E54).unwrap_or(0);
        let playtime = u32(payload, 0x33F80).unwrap_or(0);
        let content_used = steam != 0 || playtime != 0;
        let used = occupancy.as_ref().map(|o| o[i]).unwrap_or(content_used);
        let detail = read_details(payload);
        out.push(SekiroSlot {
            index: i,
            used,
            playtime_secs: playtime,
            journey: u8(payload, 0x33F34).unwrap_or(0),
            sen: u32(payload, 0x344D0),
            attack: ranged(u32(payload, 0x3449C), 1, 99),
            vitality: ranged(u32(payload, 0x34498), 1, 20),
            hp: twin(payload, 0x34470, 0x34474),
            posture: twin(payload, 0x3448C, 0x34490),
            gourd: match u8(payload, 0x34562) {
                Some(g) if (1..=10).contains(&g) => Some(g),
                _ => None,
            },
            ..detail
        });
    }
    Ok(out)
}

/// Everything that needs the item lists and the event flags.
fn read_details(payload: &[u8]) -> SekiroSlot {
    let mut skill_points = 0u32;
    let mut memories_held = 0u32;
    let mut carried = 0usize;
    let mut stored = 0usize;
    let mut arts = 0usize;
    let mut prosth = 0usize;
    let mut skills = 0usize;
    let mut cons = 0usize;
    let mut keys = 0usize;
    let mut upg = 0usize;
    let mut beads = 0usize;
    for (which, start, len) in [
        ("inv", 0x8F70Cu32, 0x7000u32),
        ("key", 0x9670Cu32, 0x2000u32),
        ("storage", 0x987A0u32, 0x9000u32),
        ("storage", 0xA1958u32, 0x4000u32),
    ] {
        let mut off = start as usize;
        let end = off + len as usize;
        while off + 16 <= end && off + 16 <= payload.len() {
            let handle = u32(payload, off).unwrap_or(0);
            if handle != 0 {
                let cat = (handle >> 28) & 0xF;
                // Only real records: an unrecognised type nibble is padding
                // or stale data, never an item.
                if cat != 0x8 && cat != 0x9 && cat != 0xB {
                    off += 16;
                    continue;
                }
                let iid = u32(payload, off + 4).unwrap_or(0) & 0x00FF_FFFF;
                let qty = u32(payload, off + 8).unwrap_or(0);
                let in_box = which == "storage";
                if in_box {
                    stored += 1;
                } else {
                    carried += 1;
                }
                match cat {
                    0x8 => {
                        if in_box {
                            // prosthetics live in inventory, not the box — it shows up there anyway
                        }
                        if iid <= 9999 {
                            arts += 1;
                        } else if (70_000..80_000).contains(&iid) {
                            prosth += 1;
                        } else {
                            skills += 1;
                        }
                    }
                    0x9 => {} // Wolf's own model parts, not gear — never shown
                    0xB => {
                        if iid == 1200 && !in_box {
                            skill_points += qty;
                        } else if (5200..=5299).contains(&iid) {
                            memories_held += 1;
                        } else if (4000..4500).contains(&iid) {
                            beads += qty as usize;
                        } else if (500..2000).contains(&iid) || (3000..4000).contains(&iid) {
                            cons += 1;
                        } else if (2000..3000).contains(&iid)
                            || (5500..6000).contains(&iid)
                            || (9000..10000).contains(&iid)
                        {
                            keys += 1;
                        } else if (6000..7000).contains(&iid) {
                            upg += 1;
                        }
                    }
                    _ => {}
                }
            }
            off += 16;
        }
    }
    let mut cats = Vec::new();
    for (label, n) in [
        ("Combat arts", arts),
        ("Prosthetics", prosth),
        ("Skills", skills),
        ("Supplies", cons),
        ("Key items", keys),
        ("Upgrade parts", upg),
        ("Beads & seeds", beads as usize),
    ] {
        if n > 0 {
            cats.push((label.to_string(), n));
        }
    }
    let bosses: Vec<String> = BOSS_FLAGS
        .iter()
        .filter(|(_, f)| flag(payload, *f).unwrap_or(false))
        .map(|(n, _)| n.to_string())
        .collect();
    let endings: Vec<String> = ENDINGS
        .iter()
        .filter(|(_, f)| flag(payload, *f).unwrap_or(false))
        .map(|(n, _)| n.to_string())
        .collect();
    let idol_areas: Vec<(String, usize, usize)> = IDOLS
        .iter()
        .map(|(area, ids)| {
            let lit = ids
                .iter()
                .filter(|(f, _)| flag(payload, *f).unwrap_or(false))
                .count();
            (area.to_string(), lit, ids.len())
        })
        .collect();
    SekiroSlot {
        index: 0,
        used: false,
        playtime_secs: 0,
        journey: 0,
        sen: None,
        attack: None,
        vitality: None,
        hp: None,
        posture: None,
        gourd: None,
        skill_points,
        memories_held,
        carried,
        stored,
        cats,
        bosses,
        endings,
        idol_areas,
    }
}

// ---------------- event flags ----------------
// Serialised exactly like Dark Souls III: ten 128-byte blocks of 1000 flags
// per category, region at slot+52. Positions verified against
// darthdemono/sl2-analyzer's measured tables.

const FLAG_REGION: usize = 52;
const FLAG_CATEGORY: usize = 0x500;
const FLAG_BLOCK: usize = 128;

fn flag_map(area: u32, sub: u32) -> Option<usize> {
    match (area, sub) {
        (10, 0) => Some(2),
        (11, 0) => Some(3),
        (11, 1) => Some(4),
        (11, 2) => Some(5),
        (13, 0) => Some(8),
        (15, 0) => Some(9),
        (17, 0) => Some(11),
        (20, 0) => Some(13),
        (25, 0) => Some(14),
        _ => None,
    }
}

fn flag(buf: &[u8], fid: u32) -> Option<bool> {
    let area = (fid / 100000) % 100;
    let sub = (fid / 10000) % 10;
    let k = if area >= 90 || area + sub == 0 {
        if fid >= 10000 {
            return None;
        }
        0
    } else if (fid / 10000000) % 10 > 5 {
        return None;
    } else {
        flag_map(area, sub)?
    };
    let n = fid % 1000;
    let off = FLAG_REGION
        + k * FLAG_CATEGORY
        + ((fid / 1000) % 10) as usize * FLAG_BLOCK
        + (n >> 5) as usize * 4
        + 3
        - ((n & 31) >> 3) as usize;
    let byte = *buf.get(off)?;
    Some(((byte >> (7 - (n & 7))) & 1) == 1)
}

const BOSS_FLAGS: [(&str, u32); 15] = [
    ("Gyoubu Masataka Oniwa", 9301),
    ("Lady Butterfly", 9302),
    ("Genichiro Ashina", 9303),
    ("Guardian Ape", 9304),
    ("Folding Screen Monkeys", 9305),
    ("Corrupted Monk", 9306),
    ("Headless Ape", 9307),
    ("Great Shinobi Owl", 9308),
    ("True Corrupted Monk", 9309),
    ("Divine Dragon", 9310),
    ("Isshin, the Sword Saint", 9312),
    ("Demon of Hatred", 9313),
    ("Emma, the Gentle Blade", 9315),
    ("Isshin Ashina", 9316),
    ("Owl (Father)", 9317),
];

const ENDINGS: [(&str, u32); 4] = [
    ("Immortal Severance", 6830),
    ("Purification", 6831),
    ("Return", 6832),
    ("Shura", 6833),
];

const IDOLS: [(&str, &[(u32, &str)]); 8] = [
    (
        "Ashina Outskirts",
        &[
            (11100000, "Dilapidated Temple"),
            (11100001, "Outskirts Wall - Gate Path"),
            (11100002, "Outskirts Wall - Stairway"),
            (11100003, "Underbridge Valley"),
            (11100004, "Ashina Castle Fortress"),
            (11100005, "Ashina Castle Gate"),
            (11100006, "Ashina Outskirts"),
            (11100007, "Flames of Hatred"),
        ],
    ),
    (
        "Hirata Estate",
        &[
            (11000000, "Dragonspring - Hirata Estate"),
            (11000001, "Estate Path"),
            (11000002, "Bamboo Thicket Slope"),
            (11000003, "Hirata Estate - Main Hall"),
            (11000004, "Hirata Estate - Hidden Temple"),
            (11000005, "Hirata Audience Chamber"),
        ],
    ),
    (
        "Ashina Castle",
        &[
            (11110000, "Ashina Castle"),
            (11110001, "Upper Tower - Antechamber"),
            (11110002, "Castle Tower Lookout"),
            (11110003, "Upper Tower - Kuro's Room"),
            (11110004, "Great Serpent Shrine"),
            (11110005, "Abandoned Dungeon Entrance"),
            (11110006, "Old Grave"),
            (11110007, "Upper Tower - Ashina Dojo"),
            (11120000, "Near Secret Passage"),
            (11120001, "Ashina Reservoir"),
        ],
    ),
    (
        "Abandoned Dungeon",
        &[
            (11300000, "Underground Waterway"),
            (11300001, "Bottomless Hole"),
        ],
    ),
    (
        "Senpou Temple",
        &[
            (12000000, "Senpou Temple, Mt. Kongo"),
            (12000001, "Shugendo"),
            (12000002, "Temple Grounds"),
            (12000003, "Main Hall"),
            (12000004, "Inner Sanctum"),
            (12000005, "Sunken Valley Cavern"),
            (12000006, "Bell Demon's Temple"),
        ],
    ),
    (
        "Sunken Valley",
        &[
            (11700000, "Sunken Valley"),
            (11700001, "Gun Fort"),
            (11700002, "Riven Cave"),
            (11700003, "Guardian Ape's Watering Hole"),
            (11700007, "Under-Shrine Valley"),
            (11700008, "Bodhisattva Valley"),
        ],
    ),
    (
        "Ashina Depths",
        &[
            (11500000, "Hidden Forest"),
            (11500001, "Mibu Village"),
            (11500002, "Water Mill"),
            (11500003, "Wedding Cave Door"),
            (11700004, "Poison Pool"),
            (11700005, "Ashina Depths"),
            (11700006, "Guardian Ape's Burrow"),
        ],
    ),
    (
        "Fountainhead Palace",
        &[
            (12500000, "Fountainhead Palace"),
            (12500001, "Vermilion Bridge"),
            (12500002, "Flower Viewing Stage"),
            (12500003, "Great Sakura"),
            (12500004, "Palace Grounds"),
            (12500005, "Sanctuary"),
            (12500006, "Mibu Manor"),
            (12500007, "Feeding Grounds"),
            (12500008, "Near Pot Noble"),
        ],
    ),
];
