# SaveVault — game save backup (Windows, Rust)

![CI](https://github.com/harshit51290/savevault/actions/workflows/ci.yml/badge.svg)
![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)

Universal save manager in the "Ember" theme: near-black blue surfaces, one
amber action color, cover art, plain words everywhere — no hex, no hashes.

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

Binary: `target\debug\savevault.exe` — double-click opens the window.

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
  Cached in `SaveVault\cache\logos\`; letter avatars where nothing found.
- **SQLite history** (`SaveVault\savevault.db`): first-seen dates, backup
  records (manual/auto/snapshot), progress snapshots per scan. Details
  shows "Tracked since …" plus the recent trail.
- **Last played** timestamps in the list and the detail view.

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

`savevault.exe --details <file>` prints everything readable in one save —
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
