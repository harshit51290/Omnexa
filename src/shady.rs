//! Shady Part of Me `save` files: plaintext `key value;` records with
//! `{ … }` blocks. Reads checkpoints, scenes and origami progress.

use anyhow::{bail, Result};

#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct ShadySave {
    pub version: String,
    pub id: String,
    pub scene: u32,
    pub checkpoint: String,
    pub max_scene: u32,
    pub max_checkpoint: String,
    pub origami: usize,
}

fn block<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let i = text.find(key)?;
    let after = &text[i + key.len()..];
    let open = after.find('{')?;
    let mut depth = 0usize;
    for (k, c) in after.char_indices() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(&after[open + 1..k]);
            }
        }
    }
    None
}

fn field(block: &str, key: &str) -> Option<String> {
    for stmt in block.split(';') {
        let s = stmt.trim();
        if let Some(rest) = s.strip_prefix(key) {
            let v = rest.trim().trim_matches('"').to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

pub fn parse(file: &[u8]) -> Result<ShadySave> {
    let text = std::str::from_utf8(file)?;
    if !text.contains("save_version") {
        bail!("not a Shady save");
    }
    let top_version = field(text, "save_version").unwrap_or_default();
    let id = field(text, "ID").unwrap_or_default();
    let mut out = ShadySave {
        version: top_version,
        id,
        ..Default::default()
    };
    for (key, is_max) in [("last_checkpoint", false), ("max_checkpoint", true)] {
        if let Some(b) = block(text, key) {
            let scene = field(&b, "scene").and_then(|s| s.parse().ok()).unwrap_or(0);
            let name = field(&b, "name").unwrap_or_default();
            if is_max {
                out.max_scene = scene;
                out.max_checkpoint = name;
            } else {
                out.scene = scene;
                out.checkpoint = name;
            }
        }
    }
    if let Some(o) = block(text, "origami") {
        // Each folded paper is a `name "…";` line inside the block.
        out.origami = o.matches("name ").count();
    }
    if out.checkpoint.is_empty() && out.max_checkpoint.is_empty() && out.origami == 0 {
        bail!("nothing readable");
    }
    Ok(out)
}
