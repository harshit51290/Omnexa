# Design — Omnexa

A locked design system for this app. Every UI change reads this file first.
Do not regenerate per view — amend this file when the system needs to grow.

## Genre

atmospheric (cinematic gaming utility)

## Macrostructure family

- App views (library, dashboard, backups, settings, about): **Workbench** —
  left nav rail + library column + dashboard, shared header/footer chrome.
- Welcome: **Centered Hero** — full-bleed banner, centered claim, live
  stats, three fit-guaranteed steps, one stacked CTA pair.

## Theme (custom, anchored on brand blue #168CFF)

Locked tokens live in `src/ui/theme.rs` — code references consts by name,
never inline hex. Mapping:

- paper / paper-2: BG `#050B14` / PANEL `#0B1628`
- card / card-hover: CARD `#101D32` / CARD_HOVER `#16263F`
- rule: BORDER `#183454`
- accent: ACCENT `#168CFF` (primary fills, ≤5% per viewport), glow CYAN `#35D7FF`
- ink / ink-2 / muted: TEXT `#EAF2FF` / WEAK `#9BAAC1` / WEAK_DIM `#63748D`
- good / warn / error: GOOD `#35D6A0` / WARN `#FFB52E` / ERROR `#FF6374`

## Typography

- Display + body: egui defaults (no bundled font files — system constraint).
  Headings roman, strong, never italic. Sizes step down with density
  (hero 24–30, card 13.5–14, meta 12–13).

## Glyph discipline (load-bearing on Windows)

egui's bundled font lacks emoji-block and enclosed glyphs — they render as
tofu boxes. Allowed: ASCII, `·…—○●◆▦▤■»✓⚠▶⟳⚙✕🛡` (verified on-screen).
Banned: `⬢ⓘ🗂📂💾🎃⚔❤✦💀👹🕯🏁🎒🪙🗡📍🦢＋` and all other pictographs.
Section voice uses `»` leaders, never emoji.

## Spacing / motion

- 8×6 item spacing, 14×8 button padding; cards radius 12–14, pills 20.
- Motion-cut: no animation libraries, spinners + state changes only.
- Reduced motion: nothing to collapse (no decorative motion ships).

## Microinteractions

- Silent success (toast line), never celebratory modals.
- Destructive/risky actions (restore, delete) confirm first; restores always
  keep a pre-restore safety copy.
- Disabled buttons carry a `why` hover explanation; status is never
  color-alone (pill text always names the state).

## CTA voice

- Primary: ACCENT fill, white bold caps, fixed size (e.g. 200×44).
- Secondary: CYAN 1.5px outline, TEXT caps, fixed size (e.g. 150×44).

## Honest-copy rule

Stats show measured values or `—`. No invented metrics, no fake totals.
Storage figures count app-managed bytes only.

## What views MUST share

Wordmark, accent + placement, CTA voice, `»` section rhythm, pill states.

## What views MAY differ on

Hero treatment (game art vs brand banner), stat-chip sets per parser.
