//! Unreal Engine `.sav` (GVAS) reader — works for any UE4/UE5 game.
//!
//! Strict on the header (magic, engine version, save class), best-effort on
//! the property list: every step is validated and the walk simply stops at
//! the first inconsistency (truncated writes happen — one file on this very
//! machine ends mid-record). What is shown was read; nothing is guessed.

use anyhow::{bail, Result};

#[derive(Clone, Default)]
pub struct GvasInfo {
    pub engine: String,
    pub class: String,
    /// (name, type) of top-level saved values.
    pub props: Vec<(String, String)>,
    /// Walk stopped early (truncated writes and UE5.6's reframed structs
    /// both land here — what was read is still shown).
    pub partial: bool,
    /// Distinct readable labels found in the file (fallback when the
    /// property walk can't proceed, e.g. UE5.6 struct framing).
    pub labels: Vec<String>,
}

struct Cursor<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> usize {
        self.b.len().saturating_sub(self.o)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.rest() < n {
            bail!("eof");
        }
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Result<i32> {
        let b = self.bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(self.i32()? as u32)
    }
    /// Unreal FString: i32 len (0 = empty, <0 = UTF-16 units incl. null,
    /// else bytes incl. trailing null).
    fn fstring(&mut self) -> Result<String> {
        let n = self.i32()?;
        if n == 0 {
            return Ok(String::new());
        }
        if n < 0 {
            let units = (-n) as usize;
            if units > 512 || self.rest() < units * 2 {
                bail!("bad string");
            }
            let mut u = Vec::with_capacity(units);
            for _ in 0..units {
                u.push(self.u16()?);
            }
            let mut s = String::from_utf16_lossy(&u);
            s.truncate(s.trim_end_matches('\0').len());
            return Ok(s);
        }
        let n = n as usize;
        if n > 4096 || self.rest() < n {
            bail!("bad string");
        }
        let raw = self.bytes(n)?;
        let mut s = String::from_utf8_lossy(&raw[..n - 1]).to_string();
        s.truncate(s.trim_end_matches('\0').len());
        Ok(s)
    }
    fn fname(&mut self) -> Result<String> {
        let s = self.fstring()?;
        if s.len() > 256
            || !s.chars().all(|c| {
                c.is_alphanumeric() || c == '_' || c == ' ' || c == '.' || c == '/' || c == '-'
            })
        {
            bail!("bad name");
        }
        Ok(s)
    }
}

pub fn parse(file: &[u8]) -> Result<GvasInfo> {
    let mut c = Cursor { b: file, o: 0 };
    if c.bytes(4)? != b"GVAS" {
        bail!("not a GVAS save");
    }
    let _save_ver = c.i32()?;
    let _pkg_ver = c.i32()?;
    // UE4: major/minor/patch triple follows immediately. UE5.6 inserts one
    // extra int32 first — detected, never assumed: a triple starting above
    // engine 9 (or at 0) means we are standing on that int.
    let mut major = c.u16()?;
    if !(1..=9).contains(&major) {
        if c.rest() < 6 {
            bail!("bad engine version");
        }
        c.o += 2; // step over the UE5 prefix int (already half-read as u16)
        major = c.u16()?;
        if !(1..=9).contains(&major) {
            bail!("unknown engine");
        }
    }
    let minor = c.u16()?;
    let patch = c.u16()?;
    let _changelist = c.u32()?;
    let branch = c.fstring()?;
    let _cv_fmt = c.i32()?;
    let cv_num = c.i32()?;
    if cv_num < 0 || cv_num > 200 {
        bail!("bad custom versions");
    }
    c.o += (cv_num as usize) * 20;
    if c.rest() < 20 {
        bail!("eof");
    }
    let class = c.fstring()?;
    if class.is_empty() || class.len() > 512 {
        bail!("bad class");
    }
    let engine = if branch.contains("++UE5") || major == 5 {
        format!("Unreal Engine 5.{minor}")
    } else {
        format!("Unreal Engine {major}.{minor}.{patch}")
    };
    // Best-effort property walk.
    let mut props = Vec::new();
    let mut partial = false;
    for _ in 0..500 {
        if c.o >= file.len() {
            break;
        }
        let mark = c.o;
        let name = match c.fname() {
            Ok(n) if n.is_empty() => break, // trailing padding
            Ok(n) => n,
            Err(_) => {
                // Ragged writers (EasySave) leave a stray byte before the
                // end marker — accept it, but only as the end.
                c.o = mark;
                if resync_end(&mut c) {
                    break;
                }
                partial = c.o < file.len();
                break;
            }
        };
        if name == "None" {
            break;
        }
        let ty = match c.fname() {
            Ok(t) => t,
            Err(_) => {
                partial = true;
                break;
            }
        };
        // UE framing: int32 size + int32 array index (identical values to
        // the old int64 read on UE4 files, correct on UE5 ones).
        let size = match c.i32() {
            Ok(s) => s,
            Err(_) => {
                partial = true;
                break;
            }
        };
        let _array = match c.i32() {
            Ok(a) => a,
            Err(_) => {
                partial = true;
                break;
            }
        };
        if size < 0 || size as usize > c.rest() {
            partial = true;
            break;
        }
        // Descend one level into structs so nested value names surface.
        if ty == "StructProperty" && props.len() < 60 {
            if (size as usize) < 16 || c.rest() < 16 {
                partial = true;
                break;
            }
            c.o += 16; // type guid
            let end = c.o + size as usize - 16;
            for _ in 0..200 {
                if c.o >= end {
                    break;
                }
                let sn = match c.fname() {
                    Ok(n) => n,
                    Err(_) => {
                        partial = true;
                        break;
                    }
                };
                if sn == "None" {
                    break;
                }
                let st = match c.fname() {
                    Ok(t) => t,
                    Err(_) => {
                        partial = true;
                        break;
                    }
                };
                let ss = match c.i32() {
                    Ok(s) => s,
                    Err(_) => {
                        partial = true;
                        break;
                    }
                };
                let _sarr = match c.i32() {
                    Ok(a) => a,
                    Err(_) => {
                        partial = true;
                        break;
                    }
                };
                if ss < 0 || ss as usize > c.rest() || c.o + ss as usize > end {
                    partial = true;
                    break;
                }
                if props.len() < 60 {
                    props.push((sn, st));
                }
                c.o += ss as usize;
            }
            c.o = end.min(file.len());
        } else {
            if props.len() < 60 {
                props.push((name, ty));
            }
            c.o += size as usize;
        }
    }
    let labels = if props.is_empty() {
        label_scan(file)
    } else {
        Vec::new()
    };
    Ok(GvasInfo {
        engine,
        class,
        props,
        partial,
        labels,
    })
}

/// Fallback when the property walk can't proceed (e.g. UE5.6's reframed
/// structs): distinct readable labels in the file. Type names, the class
/// path and engine branch are filtered out — what remains are the game's
/// own value names.
fn label_scan(file: &[u8]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut o = 0usize;
    while o + 4 < file.len() && out.len() < 40 {
        let n = i32::from_le_bytes([file[o], file[o + 1], file[o + 2], file[o + 3]]);
        if n > 2 && n <= 80 {
            let end = o + 4 + n as usize;
            if end <= file.len() {
                let raw = &file[o + 4..end];
                if raw.ends_with(b"\x00")
                    && raw[..raw.len() - 1].iter().all(|b| {
                        b.is_ascii_alphanumeric()
                            || *b == b'_'
                            || *b == b' '
                            || *b == b'.'
                            || *b == b'/'
                    })
                {
                    let s = String::from_utf8_lossy(&raw[..raw.len() - 1]).to_string();
                    if s.len() >= 3
                        && !s.ends_with("Property")
                        && s != "None"
                        && !s.starts_with("++UE")
                        && seen.insert(s.clone())
                    {
                        out.push(s);
                    }
                    o = end;
                    continue;
                }
            }
        }
        o += 1;
    }
    out
}

/// A stray byte before the end marker (EasySave's terminator quirk):
/// accept it only when "None" follows within a few bytes and almost
/// nothing trails it. Anything else is real damage, not a quirk.
fn resync_end(c: &mut Cursor) -> bool {
    let save = c.o;
    for d in 1..=4 {
        c.o = save + d;
        if let Ok(n) = c.fname() {
            if n == "None" && c.rest() <= 8 {
                return true;
            }
        }
    }
    c.o = save;
    false
}
