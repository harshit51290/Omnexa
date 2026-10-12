# Contributing to Omnexa

Thanks for helping out. Small, honest PRs beat big clever ones here.

## Ground rules

1. **No jargon in the UI.** Every label must make sense to someone who has
   never opened a hex editor. Paths, hashes, offsets and format names stay
   in `Details`, logs and `--details`.
2. **Parsers never crash on user files.** Every read is bounds-checked;
   unknown data loses fields, never panics. A wrong number is worse than
   a missing one — drop the field and say so.
3. **Verify against real saves.** New offsets need a before/after pair or
   a second independent source. `omnexa.exe --details <file>` is the
   harness — paste its output in the PR.
4. **No game-specific hardcoding for discovery.** New games must be found
   by the universal detector (install sources, save matching, signatures).
   Format parsers keyed by file signature are fine.
5. **No DRM bypass, no bundled game assets.** Backup/restore/resign of
   your own saves only. Never commit `.sl2`/`.dat` files, keys that ship
   inside games, or emulator binaries.

## Build

Windows, Rust stable. MSVC needs the Windows SDK; without it use the GNU
toolchain (the checked-in `.cargo/config.toml` carries the MinGW link
path for that setup):

```powershell
cargo build
cargo run -- --scan       # list detected games, no window
cargo run -- --selftest   # full pipeline: discover + art + database
cargo run -- --details <save file>
```

Please keep builds warning-free (`cargo build` should end clean).

## PR process

- Fork, branch from `main`, one topic per PR.
- Include what you tested on (game, file, `--details` output).
- UI changes: describe the before/after in words (screenshots welcome).
- Be kind in review. First-time contributors get extra patience.
