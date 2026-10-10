//! Hollow Knight (and Silksong) saves: BinaryFormatter-wrapped, base64'd,
//! AES-128-ECB JSON. Key and layout from community save editors
//! (Microflame's gist, Bloodorca's hollow, nakami's Save-Crypto),
//! verified against a real user1.dat on this machine.

use anyhow::{bail, Result};

const HEADER: [u8; 22] = [
    0, 1, 0, 0, 0, 255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 0, 0, 6, 1, 0, 0, 0,
];
const KEY: &[u8; 32] = b"UKu52ePUBwetZ9wNX88o54dnfKRu0T1l";

#[derive(Clone, Default)]
pub struct HollowSave {
    pub health: Option<u64>,
    pub max_health: Option<u64>,
    pub geo: Option<u64>,
    pub play_secs: Option<u64>,
    pub nail: Option<u64>,
    pub completion: Option<f64>,
    pub version: Option<String>,
}

fn num(v: &serde_json::Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return u64::try_from(n).ok();
    }
    v.as_f64().map(|f| f as u64)
}

pub fn parse(file: &[u8]) -> Result<HollowSave> {
    if file.len() < HEADER.len() + 2 || file[..HEADER.len()] != HEADER {
        bail!("not a Hollow Knight save");
    }
    let mut body = &file[HEADER.len()..file.len() - 1];
    // .NET 7-bit length prefix
    let mut len: usize = 0;
    let mut shift = 0;
    loop {
        if body.is_empty() || shift > 35 {
            bail!("bad length prefix");
        }
        let b = body[0];
        body = &body[1..];
        len |= ((b & 0x7F) as usize) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    if body.len() < len {
        bail!("truncated save");
    }
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &body[..len])?;
    if raw.len() % 16 != 0 || raw.is_empty() {
        bail!("bad crypto block");
    }
    use aes::cipher::{BlockDecrypt, KeyInit};
    let cipher = aes::Aes256::new_from_slice(KEY).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut pt = raw;
    for chunk in pt.chunks_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }
    let pad = *pt.last().unwrap() as usize;
    if pad == 0 || pad > 16 || pad > pt.len() {
        bail!("bad padding");
    }
    pt.truncate(pt.len() - pad);
    let text = std::str::from_utf8(&pt)?;
    let json: serde_json::Value = serde_json::from_str(text)?;
    let pd = &json["playerData"];
    Ok(HollowSave {
        health: num(&pd["health"]),
        max_health: num(&pd["maxHealth"]),
        geo: num(&pd["geo"]),
        play_secs: num(&pd["playTime"]),
        nail: num(&pd["nailDamage"]),
        completion: pd["completionPercentage"].as_f64(),
        version: pd["version"].as_str().map(|s| s.to_string()),
    })
}

pub fn fmt_playtime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}
