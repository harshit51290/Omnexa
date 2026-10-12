//! What's inside an Elden Ring save slot: roster names + stat block.
//!
//! Layout verified against darthdemono/sl2-analyzer (MIT) and checked against
//! a real ER0000.sl2 on this machine. The stat block moves per character, so
//! it is *found* by the game's own level formula
//! (level == vigor+...+arcane - 79), never trusted by position.

use anyhow::Result;

const ATTRS: [&str; 8] = ["VIG", "MND", "END", "STR", "DEX", "INT", "FAI", "ARC"];
const ATTR_OFFS: [usize; 8] = [0, 4, 8, 12, 16, 20, 24, 28];

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

fn read_name(menu: &[u8], off: usize) -> Option<String> {
    if off + 32 > menu.len() {
        return None;
    }
    let mut units = Vec::new();
    for i in 0..16 {
        let c = u16::from_le_bytes([menu[off + i * 2], menu[off + i * 2 + 1]]);
        if c == 0 {
            break;
        }
        units.push(c);
    }
    let s = String::from_utf16_lossy(&units);
    let s = s.trim().to_string();
    if s.is_empty() || s.len() > 32 {
        return None;
    }
    // Must look like a name, not binary noise.
    if !s
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '\'' || c == '_' || c == '.')
    {
        return None;
    }
    Some(s)
}

#[derive(Clone)]
pub struct ErSlot {
    pub index: usize,
    pub active: bool,
    pub name: Option<String>,
    pub level: u32,
    pub runes: Option<u32>,
    pub hp: Option<u32>,
    pub attrs: Option<[u32; 8]>,
}

pub fn attr_names() -> [&'static str; 8] {
    ATTRS
}

/// Parse roster (menu entry 10, checksum included) + per-slot stats.
pub fn parse(file: &[u8]) -> Result<Vec<ErSlot>> {
    let entries = crate::parsers::bnd4::entries(file)?;
    if entries.len() < 11 {
        anyhow::bail!("no menu entry");
    }
    let menu = &file[entries[10].data_off..];
    let len = u32(menu, 352).unwrap_or(0) as usize;
    let active_base = 356 + len;
    let prof_base = active_base + 10;
    let mut out = Vec::new();
    for i in 0..10 {
        let active = menu.get(active_base + i).copied().unwrap_or(0) == 1;
        let base = prof_base + i * 588;
        let name = read_name(menu, base);
        let level = u32(menu, base + 34).unwrap_or(0);
        let (runes, hp, attrs) = if active && (1..=713).contains(&level) {
            let end = entries.get(i + 1).map(|n| n.data_off).unwrap_or(file.len());
            match file.get(entries[i].data_off + 16..end) {
                Some(payload) => find_stats(payload, level),
                None => (None, None, None),
            }
        } else {
            (None, None, None)
        };
        out.push(ErSlot {
            index: i,
            active,
            name,
            level,
            runes,
            hp,
            attrs,
        });
    }
    Ok(out)
}

fn find_stats(buf: &[u8], level: u32) -> (Option<u32>, Option<u32>, Option<[u32; 8]>) {
    let pat = level.to_le_bytes();
    let mut at = find(buf, &pat, 0);
    while let Some(pos) = at {
        let v = pos as isize - 44;
        if v >= 0 {
            let v = v as usize;
            let mut vals = [0u32; 8];
            let mut ok = true;
            for (k, d) in ATTR_OFFS.iter().enumerate() {
                match u32(buf, v + d) {
                    Some(x) if (1..=99).contains(&x) => vals[k] = x,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && vals.iter().sum::<u32>() == level + 79 {
                let hp = u32(buf, v.wrapping_sub(40));
                let runes = u32(buf, v + 48);
                return (runes, hp, Some(vals));
            }
        }
        at = find(buf, &pat, pos + 1);
    }
    (None, None, None)
}

fn find(buf: &[u8], pat: &[u8; 4], from: usize) -> Option<usize> {
    if from + 4 > buf.len() {
        return None;
    }
    buf[from..]
        .windows(4)
        .position(|w| w == pat)
        .map(|p| from + p)
}
