# Save types — which games share Sekiro's format

## Group A — Same type as Sekiro (FromSoftware BND4 `.sl2`)

Verified by header read: magic `BND4`, entry count at offset `0x0C`.

| Save | Size | Entries | Layout |
|---|---|---|---|
| Sekiro `S0000.sl2` | 11,928,528 | 12 | 10x `USER_DATA000-009` (1 MB each) + `USER_DATA010` (384 KB) + `USER_DATA011` (~1 MB) |
| Elden Ring `ER0000.sl2` | 28,967,888 | 12 | Same 10+2 pattern, bigger slots |
| Nightreign `NR0000.sl2` | 19,531,312 | 14 | Same family, 14 entries (2 extra slots/data) |

Rules for all three:
- File = `BND4` container, names in UTF-16LE (`USER_DATAxxx`).
- Every chunk = 16-byte MD5 + data. Must recalc MD5 after edit.
- Game keeps `.sl2` + identical `.sl2.bak`. Copy/restore both, game closed.
- SteamID folder + SteamID bytes inside file.

One parser handles all of Group A. Only slot size / entry count differs.

## Group B — Unreal Engine `.sav` (Choo-Choo Charles)

- `%LOCALAPPDATA%\Obscure\Saved\SaveGames\*.sav`, ~1.3 KB each, one file per checkpoint.
- Opaque copy: back up the whole `SaveGames` folder. No checksum work needed.

## Group C — Unity / Team Cherry (Hollow Knight)

- `user1.dat` + `user1.dat.bak1`, ~74 KB.
- Opaque copy of both files. Silksong will use the same folder pattern once played.

## Group D — Ubisoft / UPLAY `.save` (Watch_Dogs 2, Far Cry 5)

- `Ubisoft Game Launcher\savegames\<user>\<gameid>\1.save`, `2.save` + `My Games\<Game>\*GamerProfile.xml`.
- Back up both locations together or the profile desyncs.

## Group E — My Games XML / DAT (Shady, SnowRunner, Farming Sim, KF2)

- Shady: `My Games\Shady Part of Me\<id>\save` + `save_backup` (7 KB).
- SnowRunner: `My Games\SnowRunner\base\...`.
- Farming Sim 2025: `My Games\FarmingSimulator2025\game.xml` + `*.dat`.
- KF2: `My Games\KillingFloor2\KFGame\Config\*.ini` (settings only).
- Plain files — zip the game folder, no parsing needed.

## Group F — Online-only (PUBG, VTube, SeizeControl)

- Progress lives on servers. Local `%LOCALAPPDATA%\TslGame\Saved\` is cache/demos/logs.
- Nothing worth backing up for progress; skip in v1.

## Group G — Not located yet

- RE7: no save played yet (only `re7_config.ini`). RE Engine normally uses `%LOCALAPPDATA%\CAPCOM\...` — will appear after first run.

## What this means for the app

1. Build the `BND4` module first — covers Sekiro + Elden Ring + Nightreign (your 3 biggest saves).
2. Everything else = "opaque folder backup" mode: copy whole save folder + restore verbatim. Covers Groups B–E with zero format work.
