![Banner](assets/banner.png)

# <img src="assets/logo.png" width="48" align="left"> Omnexa — Universal Data Saver (Windows, Rust)

![CI](https://github.com/harshit51290/Omnexa/actions/workflows/ci.yml/badge.svg)
![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)

One system. Smarter data. Omnexa finds your games, shows what's inside
their saves in plain words, and keeps verified backups — neon gaming
library UI, no hex, no hashes, nothing uploaded anywhere.

## Run

MSVC linker needs the Windows SDK (not installed here), so build with the
GNU toolchain (scoop `gcc`). Link path is already in `.cargo/config.toml`.

```powershell
cargo +stable-x86_64-pc-windows-gnu run
# headless checks, no window:
cargo +stable-x86_64-pc-windows-gnu run -- --scan      # list games
cargo +stable-x86_64-pc-windows-gnu run -- --selftest  # full pipeline: discover + art + database
cargo +stable-x86_64-pc-windows-gnu run -- --details <save file>
```

Binary: `target\debug\omnexa.exe` — double-click opens the window.

## How it finds games (no hardcoded list)

Installs: Steam libraries (all drives + `libraryfolders.vdf`), Ubisoft /
Epic / GOG folders, Epic manifests, the Windows uninstall registry, and
plain `C:\` / `D:\` exe roots — plus folders you add in Settings.
Saves: name-matched homes under Roaming / Local / LocalLow / Documents /
My Games, exe-named folders (UE4/Unity pattern), in-install save dirs, and
orphan save folders with no install left. Duplicates pointing at the same
saves are merged. FromSoftware saves are recognised by signature
(`S0000`/`ER0000`/`NR0000`, content-sniffed fallback).

## Cover art + history

- **Art, automatic**: Steam library headers → SteamGridDB grids (paste a
  free key from steamgriddb.com in Settings) → each game's own exe icon.
  Cached in `Omnexa\cache\logos\`; letter avatars where nothing found. Early builds used `Documents\SaveVault` — if that folder exists, it keeps being used so no backup is ever orphaned.
- **SQLite history** (`Omnexa\omnexa.db`, auto-migrated from `savevault.db`): first-seen dates, backup
  records (manual/auto/snapshot), progress snapshots per scan. Details
  shows "Tracked since …" plus the recent trail.
- **Last played** timestamps in the list and the detail view.
- **Storage layout (SHA-256)**: game folders are
  `<12-hex-of-install-key>-<readable-name>`, backup folders are
  `<timestamp>-<8-hex>`, stored files are `<16-hex-of-source-path>_<name>.bin`.
  Short, deterministic, collision-free — long Windows save paths can never
  break a backup. Every file carries a SHA-256 in `manifest.json`; restores
  refuse corrupted copies. Old pre-hash backups are still found and restored.

## Experience

- Cinematic blue library theme (`#050B14` → `#101D32`, electric `#168CFF`,
  cyan `#35D7FF`): nav rail (Games / Backups / Settings / About), live
  vault stats, cover-first cards, hero banners, stat chips.
- Filter chips carry live counts (All / Protected / Needs backup / Running),
  skeleton rows while scanning, welcome screen on first run.
- Game cards show protection pills + running badges + batch-select
  checkboxes; “Back up N selected” backs them all up in one background job.
- Storage card shows honest app-managed bytes (backups vs saves ring) —
  never invented disk totals.
- Dedicated Backups page: every backup of every game with restore, folder
  reveal, and arm-then-confirm delete. About page with version + credits.
- Backup history lists every entry with date, size and ✓ verified badge;
  each row has its own Restore + Folder buttons. Restores ask for
  confirmation and always keep a pre-restore safety copy.
- Backups and restores run off the UI thread with a Working… indicator;
  results arrive as toasts, never a lost status line.
- Settings are sectioned (Backups / Cover art / Auto-protect / Folders)
  with live disk-usage readout.

## Codebase layout

```
src/main.rs          tiny entry: mods + window setup + headless dispatch
src/core/            backend: games (discovery), backup (engine),
                     db (SQLite index), logos (cover art), settings
src/parsers/         save readers: sekiro, elden, hollow, gvas,
                     minecraft, shady (+ bnd4 container)
src/ui/              app (shell, views, jobs), theme (tokens, badges,
                     logo mark, storage ring), cards (library + save cards)
```

## What it understands inside saves

- **Sekiro**: journey, playtime, Sen, Attack (+named bosses beaten),
  Vitality (+necklaces), Health, Posture, Gourd (+seeds), skill points,
  memories held, inventory counts, idols lit per area (20/55 tracked),
  ending reached. Offsets follow darthdemono/sl2-analyzer's documented
  research, verified byte-for-byte against the saves on this machine.
- **Elden Ring**: roster names + levels, runes, Health, 8 attributes —
  found by the game's own level formula, shown only when the math checks.
- **Hollow Knight / Silksong**: masks, Geo, nail, playtime (AES save
  format decoded with the community-published key).
- **Unreal Engine (any game)**: engine version + save class for UE4/UE5,
  saved values for standard layouts, "values mentioned" fallback for
  reframed UE5 structs; checkpoint filenames are the content for
  checkpoint-style games.
- **Minecraft Java**: world name, mode, playtime, cheats (NBT `level.dat`).
- **Shady Part of Me**: scene, checkpoint, furthest progress, origami.
- **Any JSON save**: readable fields preview.
- **Nightreign**: slots are encrypted, so nothing is claimed — backup
  still works normally.
- Anything else: honest file counts, backed up verbatim.

`omnexa.exe --details <file>` prints everything readable in one save —
handy for checking a backup before you trust it.

## Edge cases handled

- **Game running**: amber banner + Restore locked while the exe runs.
- **Restore safety**: current state is snapshotted (`pre-restore-…`)
  before anything is overwritten; `.sl2` fingerprints re-signed after.
- **Unclean shutdown**: `.sl2` vs `.sl2.bak` twin mismatch warns you.
- **Steam Cloud**: restore screen reminds you to pause it while testing.
- **Retention**: keeps the last N backups per game (Settings, default 8).
- **Errors in plain words**: access-denied → run-as-admin hint, disk
  full, moved files, locked files — never a stack trace.
- **Non-games filtered**: launchers, drivers, chat apps and IDE caches
  are excluded by evidence rules (real save formats required), and games
  appear once they actually have saves.
- Emulator traces (Goldberg/CPY/SmartSteamLoader) are never bundled.
- See `GAMES.md` / `SAVE_TYPES.md` for the original research notes.

## Attribution

Save-format research stands on community work: Sekiro/Elden Ring field
positions follow
[darthdemono/sl2-analyzer](https://github.com/darthdemono/sl2-analyzer)'s
documented offsets (MIT); Hollow Knight's save crypto follows the
community-published key and layout (Microflame's gist, Bloodorca's
`hollow`, nakami's Save-Crypto); Unreal GVAS framing follows public
engine knowledge. Item-name tables are not shipped — the app reads
counts and player-facing stats only.

## Contributing & license

MIT — see [LICENSE](LICENSE). Bug reports and PRs welcome; please read
[CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) first
(never attach full save files to issues — they contain your Steam ID).
