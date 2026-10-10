# Detected games on this PC

Scanned registry + `C:\` roots + Steam `common` + `%APPDATA%` / `%LOCALAPPDATA%` / `%LOCALLOW%` / Documents.

| Game | Install root | Exe | Save found |
|---|---|---|---|
| Sekiro: Shadows Die Twice | `C:\Sekiro - Shadows Die Twice\` | `sekiro.exe` | Yes — `%APPDATA%\Sekiro\<steam-id>\S0000.sl2` (11,928,528 B) + `.bak` |
| ELDEN RING | `C:\ELDEN RING\` | launcher only in root | Yes — `%APPDATA%\EldenRing\<steam-id>\ER0000.sl2` (28,967,888 B) + `.bak` |
| Nightreign (Elden Ring) | no separate root | — | Yes — `%APPDATA%\Nightreign\<steam-id>\NR0000.sl2` (19,531,312 B) + `.bak` |
| Hollow Knight | `C:\Hollow Knight\` | `hollow_knight.exe` | Yes — `%LOCALLOW%\Team Cherry\Hollow Knight\user1.dat` (74,098 B) + `.bak1` |
| Hollow Knight: Silksong | `C:\Hollow Knight - Silksong\` | `Hollow Knight Silksong.exe` | Folder exists, no `.dat` yet — only logs |
| Choo-Choo Charles (Obscure, UE4) | `C:\Choo-Choo Charles\` | `Obscure.exe` | Yes — `%LOCALAPPDATA%\Obscure\Saved\SaveGames\*.sav` (1,295 B each, many checkpoints) |
| Resident Evil 7 | `C:\Resident Evil 7 - Biohazard\` | `re7.exe` | Not played yet — no `CAPCOM` save folder, only `re7_config.ini` in root |
| Shady Part of Me | `C:\Shady Part of Me\` | `spom.exe` | Yes — `Documents\My Games\Shady Part of Me\685041355\save` (7,381 B) + `save_backup` |
| Watch_Dogs 2 | `...\Steam\steamapps\common\Watch_Dogs2` | Steam | Yes — Ubisoft `savegames\...\3619\1.save` (5,624 B), `2.save` (38,387 B) + `My Games\Watch_Dogs 2\WD2_GamerProfile.xml` |
| Far Cry 5 | — (Ubisoft) | — | Yes (config) — `My Games\Far Cry 5\gamerprofile.xml` |
| Farming Simulator 2025 | — | — | Yes — `My Games\FarmingSimulator2025\game.xml`, `gameSettings.xml`, `*.dat` |
| SnowRunner | — | — | Yes — `My Games\SnowRunner\base\...` |
| Killing Floor 2 | `...\Steam\steamapps\common\killingfloor2` | Steam (`Binaries`) | Config only — `My Games\KillingFloor2\KFGame\Config\*.ini` (progress is online) |
| PUBG | `...\Steam\steamapps\common\PUBG` | `TslGame` | Online-only — local `%LOCALAPPDATA%\TslGame\Saved\` is cache/demos/logs, no local progress |
| VTube Studio / SeizeControl | Steam `common` | Steam | Config-only, skip for backup v1 |
| Minecraft (TLauncher) | `%APPDATA%\.minecraft` | TLauncher | Worlds in `.minecraft\saves` (not scanned yet) |

Emulator traces (do not bundle in app): `%APPDATA%\Goldberg SteamEmu Saves\`, `Documents\CPY_SAVES\`, `steam_emu.ini` / `SmartSteamLoader` / `steam_settings\` inside game roots.
