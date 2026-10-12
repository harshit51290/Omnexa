//! Minecraft Java `level.dat`: gzip'd NBT. Minimal reader — world name,
//! mode, playtime, cheats. Full tag-skipping for all 12 NBT types so
//! unknown tags never break the read.

use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use std::io::Read;

#[derive(Clone, Default)]
pub struct McWorld {
    pub name: String,
    pub mode: String,
    pub play_mins: u64,
    pub cheats: bool,
}

struct Cursor {
    b: Vec<u8>,
    o: usize,
}

impl Cursor {
    fn rest(&self) -> usize {
        self.b.len().saturating_sub(self.o)
    }
    fn raw(&mut self, n: usize) -> Result<&[u8]> {
        if self.rest() < n {
            bail!("eof");
        }
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.raw(1)?[0])
    }
    fn u16(&mut self) -> Result<usize> {
        let b = self.raw(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]) as usize)
    }
    fn i32(&mut self) -> Result<i32> {
        let b = self.raw(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i64(&mut self) -> Result<i64> {
        let b = self.raw(8)?;
        Ok(i64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn name(&mut self) -> Result<String> {
        let n = self.u16()?;
        if n > 512 {
            bail!("bad name");
        }
        Ok(String::from_utf8_lossy(self.raw(n)?).to_string())
    }
    fn skip(&mut self, t: u8) -> Result<()> {
        match t {
            1 => {
                self.raw(1)?;
            }
            2 => {
                self.raw(2)?;
            }
            3 | 5 => {
                self.raw(4)?;
            }
            4 | 6 => {
                self.raw(8)?;
            }
            7 => {
                let n = self.i32()?;
                if n < 0 || n as usize > self.rest() {
                    bail!("eof");
                }
                self.o += n as usize;
            }
            8 => {
                let n = self.u16()?;
                self.raw(n)?;
            }
            9 => {
                let et = self.u8()?;
                let n = self.i32()?;
                if n < 0 || n > 100_000 {
                    bail!("bad list");
                }
                for _ in 0..n {
                    if et == 10 {
                        self.compound()?;
                    } else {
                        self.skip(et)?;
                    }
                }
            }
            10 => {
                self.compound()?;
            }
            11 => {
                let n = self.i32()?;
                if n < 0 || n as usize * 4 > self.rest() {
                    bail!("eof");
                }
                self.o += n as usize * 4;
            }
            12 => {
                let n = self.i32()?;
                if n < 0 || n as usize * 8 > self.rest() {
                    bail!("eof");
                }
                self.o += n as usize * 8;
            }
            _ => bail!("bad tag"),
        }
        Ok(())
    }
    fn compound(&mut self) -> Result<Vec<(String, u8, Val)>> {
        let mut out = Vec::new();
        for _ in 0..5000 {
            let t = self.u8()?;
            if t == 0 {
                return Ok(out);
            }
            let n = self.name()?;
            let v = self.value(t)?;
            out.push((n, t, v));
        }
        bail!("compound too big")
    }
    fn value(&mut self, t: u8) -> Result<Val> {
        match t {
            1 => Ok(Val::Int(self.u8()? as i64)),
            2 => {
                let b = self.raw(2)?;
                Ok(Val::Int(i16::from_be_bytes([b[0], b[1]]) as i64))
            }
            3 => Ok(Val::Int(self.i32()? as i64)),
            4 => Ok(Val::Int(self.i64()?)),
            5 => {
                let b = self.raw(4)?;
                Ok(Val::Float(
                    f32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64
                ))
            }
            6 => {
                let b = self.raw(8)?;
                Ok(Val::Float(f64::from_be_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ])))
            }
            8 => {
                let n = self.u16()?;
                Ok(Val::Str(String::from_utf8_lossy(self.raw(n)?).to_string()))
            }
            10 => {
                let inner = self.compound()?;
                Ok(Val::Map(
                    inner.into_iter().map(|(n, _, v)| (n, v)).collect(),
                ))
            }
            _ => {
                self.skip(t)?;
                Ok(Val::Skip)
            }
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
enum Val {
    Int(i64),
    Float(f64),
    Str(String),
    Map(std::collections::HashMap<String, Val>),
    Skip,
}

pub fn parse(file: &[u8]) -> Result<McWorld> {
    let mut dec = GzDecoder::new(file);
    let mut raw = Vec::new();
    dec.read_to_end(&mut raw)?;
    if raw.len() < 3 || raw[0] != 10 {
        bail!("not NBT");
    }
    let mut c = Cursor { b: raw, o: 1 };
    // Root layout: [0A][name_len u16][name][compound]. Name is empty.
    let rn = c.u16()?;
    c.o += rn;
    let top = c.compound()?;
    let data = top
        .iter()
        .find(|(n, _, _)| n == "Data")
        .and_then(|(_, _, v)| match v {
            Val::Map(m) => Some(m.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let get = |k: &str| data.get(k).cloned().unwrap_or(Val::Skip);
    let name = match get("LevelName") {
        Val::Str(s) if !s.is_empty() => s,
        _ => "Unnamed world".into(),
    };
    let ticks = match get("Time") {
        Val::Int(t) if t > 0 => t as u64,
        _ => 0,
    };
    let (mode, hardcore) = match get("GameType") {
        Val::Int(1) => ("Creative", false),
        Val::Int(2) => ("Adventure", false),
        Val::Int(3) => ("Spectator", false),
        _ => ("Survival", matches!(get("hardcore"), Val::Int(1))),
    };
    let mode = if hardcore {
        "Hardcore".to_string()
    } else {
        mode.to_string()
    };
    let cheats = matches!(get("allowCommands"), Val::Int(1));
    Ok(McWorld {
        name,
        mode,
        play_mins: ticks / 20 / 60,
        cheats,
    })
}
