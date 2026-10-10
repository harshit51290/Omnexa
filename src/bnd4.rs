use anyhow::{bail, Result};
use std::path::Path;

#[allow(dead_code)]
#[derive(Clone)]
pub struct Bnd4Slot {
    pub name: String,
    pub data_len: u64,
    pub md5_ok: bool,
    pub active: bool,
    /// File offset of the 16-byte MD5 that prefixes this entry's data.
    pub data_off: usize,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct Bnd4Info {
    pub file_count: u32,
    pub slots: Vec<Bnd4Slot>,
    pub all_ok: bool,
}

/// Raw BND4 entry table: names + data offsets. Shared by all game parsers.
#[allow(dead_code)]
#[derive(Clone)]
pub struct RawEntry {
    pub name: String,
    pub data_off: usize,
}

pub fn entries(file: &[u8]) -> Result<Vec<RawEntry>> {
    if file.len() < 0x40 || &file[0..4] != b"BND4" {
        bail!("not a BND4 file");
    }
    let count = read_u32le(file, 0x0C)? as usize;
    if count == 0 || count > 64 {
        bail!("bad entry count {count}");
    }
    let mut out = Vec::new();
    for i in 0..count {
        let eoff = 0x40 + i * 0x20;
        let data_off = read_u32le(file, eoff + 16)? as usize;
        let name_off = read_u32le(file, eoff + 20)? as usize;
        out.push(RawEntry {
            name: read_name(file, name_off),
            data_off,
        });
    }
    Ok(out)
}

fn read_u32le(d: &[u8], off: usize) -> Result<u32> {
    if off + 4 > d.len() {
        bail!("short read");
    }
    Ok(u32::from_le_bytes([
        d[off],
        d[off + 1],
        d[off + 2],
        d[off + 3],
    ]))
}

fn read_name(d: &[u8], mut off: usize) -> String {
    let mut u16s = Vec::new();
    loop {
        if off + 1 >= d.len() {
            break;
        }
        let c = u16::from_le_bytes([d[off], d[off + 1]]);
        if c == 0 {
            break;
        }
        u16s.push(c);
        off += 2;
        if u16s.len() > 64 {
            break;
        }
    }
    String::from_utf16_lossy(&u16s)
}

#[allow(dead_code)]
pub fn parse_sl2(path: &Path) -> Result<Bnd4Info> {
    let d = std::fs::read(path)?;
    if d.len() < 0x40 || &d[0..4] != b"BND4" {
        bail!("not a BND4 file");
    }
    let count = read_u32le(&d, 0x0C)?;
    if count == 0 || count > 64 {
        bail!("bad entry count {count}");
    }
    let mut slots = Vec::new();
    let mut all_ok = true;
    for i in 0..(count as usize) {
        let eoff = 0x40 + i * 0x20;
        let data_off = read_u32le(&d, eoff + 16)? as usize;
        let name_off = read_u32le(&d, eoff + 20)? as usize;
        let next_off = if i + 1 < count as usize {
            read_u32le(&d, eoff + 0x20 + 16)? as usize
        } else {
            d.len()
        };
        if data_off + 16 > d.len() || next_off > d.len() || next_off < data_off {
            bail!("bad entry offsets");
        }
        let stored = &d[data_off..data_off + 16];
        let blob = &d[data_off + 16..next_off];
        let calc = md5::compute(blob);
        let ok = stored == calc.0;
        all_ok &= ok;
        let active =
            blob.len() >= 4 && i32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) != 0;
        slots.push(Bnd4Slot {
            name: read_name(&d, name_off),
            data_len: blob.len() as u64,
            md5_ok: ok,
            active,
            data_off,
        });
    }
    Ok(Bnd4Info {
        file_count: count,
        slots,
        all_ok,
    })
}

/// Recalculate every per-chunk MD5 in place (required after any edit).
pub fn resign_sl2(path: &Path) -> Result<bool> {
    let mut d = std::fs::read(path)?;
    if d.len() < 0x40 || &d[0..4] != b"BND4" {
        bail!("not a BND4 file");
    }
    let count = read_u32le(&d, 0x0C)? as usize;
    let mut changed = false;
    for i in 0..count {
        let eoff = 0x40 + i * 0x20;
        let data_off = read_u32le(&d, eoff + 16)? as usize;
        let next_off = if i + 1 < count {
            read_u32le(&d, eoff + 0x20 + 16)? as usize
        } else {
            d.len()
        };
        let blob = d[data_off + 16..next_off].to_vec();
        let calc = md5::compute(&blob).0;
        if d[data_off..data_off + 16] != calc {
            d[data_off..data_off + 16].copy_from_slice(&calc);
            changed = true;
        }
    }
    if changed {
        std::fs::write(path, &d)?;
    }
    Ok(changed)
}
