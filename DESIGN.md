# DESIGN.md

Visual direction for the Owl UI redesign (Owl started as a fork of Handy). Read this before changing colors, surfaces, or component styling. Token values live in `src/styles/theme.css`; this file says which values to use and why.

**Visual reference:** the maintainer's design boards in [Paper](https://paper.design) aren't public. This file and `src/styles/theme.css` are the source of truth for everyone else. To see the components in both themes, run `bun run dev` and open `http://localhost:1420/?mock`.

**Status:** agreed 2026-10-06; migration steps 1–3 implemented 2026-10-07 (step 4, the rename, is skipped). See "Migration".

## References

| Reference                                                                                     | What we take                                                                                                                                                                                      | What we leave                                                                                   |
| --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| [Superwhisper](https://superwhisper.com/#pricing)                                             | Near-black canvas, one signature gradient used sparingly, pill controls (segmented toggle), near-monochrome UI                                                                                    | Marketing-scale type, 20px card radius, white-pill primary button, the rainbow of button colors |
| [T3 Code](https://github.com/pingdotgg/t3code) (`apps/web/src/index.css` and the desktop app) | The blue accent and its sidebar-header gradient, zinc/neutral surface hierarchy, borders-not-fills in dark mode, inset top highlight on filled controls, dense type scale, semantic status tokens | Glass and blur effects                                                                          |

## Color analysis

### Superwhisper

Superwhisper's UI is nearly monochrome: its main CTA is a white pill with black text. Color appears in two places:

- **Signature gradient** on the highlighted Pro card: `#050116 → #112d72 (30%) → #4b68aa (45%) → #87a7dc (64%) → #9cacd0 (88%) → #fcdbef (98%)`.
- **Badges**: a purple chip, `#5c1694` background with `#fd99ff` text.

Surfaces (dark only): page `#000000`, card `#0f0f10` with a 1px `#1c1d1f` border, secondary button `#1c1d1f`, muted text `#888b91`.

### T3 Code

The stylesheet's primary is `oklch(0.488 0.217 264)` ≈ `#1b4ed8` light and `oklch(0.571 0.21 264)` ≈ `#346bf1` dark, used for the send button, focus rings and update badges. The desktop app shows a brighter sky blue: its sidebar header glows `#182676 → #1f4195 → #3269bd → #70b1ff`, and its badges sit around `#63a6f6`. Everything else is neutral:

- Light: background `zinc-25` (~`#fcfcfc`), card white, border `zinc-200`, text `zinc-800`, muted `zinc-500`.
- Dark: background `neutral-950` (`#0a0a0a`), card at 3% white, borders at 6% white, hovers at 4% white. Borders and hover states separate surfaces "without milky gray fills".

### What they share

Both are roughly 95% neutral and spend one accent on the most important action. Both lean blue. Neither uses pink.

## Direction: neutral shell, T3 Sky accent, no pink

1. **No pink, anywhere.** Not in controls, the logo, the tray icon, the overlay or illustrations.
2. **Neutrals do the work.** Surfaces, borders and text follow T3 Code's zinc hierarchy. The warm-gray dark background `#2c2b29` becomes near-black `#0a0a0a`.
3. **One accent, T3 Sky, in two strengths.** A deeper _fill_ blue for primary buttons and toggles, and a brighter _text_ blue for links, focus rings, selected states, the recording meter and the logo. One accent element per view is the target; three is the ceiling.
4. **Deep fills with white text in both themes.** Buttons stay deep blue in dark mode, which reads more premium than a bright pill. Filled buttons get T3 Code's 1px inset top highlight.
5. **One signature gradient,** sampled from T3 Code's sidebar header. Hero moments only.

Superwhisper Indigo (`#4b68aa` / `#87a7dc`) and Electric Indigo (`#4f46e5` / `#818cf8`) were drawn and set aside; they remain on the accent-options board.

## Tokens

Target values. Contrast ratios are WCAG, measured against the canvas unless noted.

| Token                        | Light                      | Dark                        | Use                                                                           |
| ---------------------------- | -------------------------- | --------------------------- | ----------------------------------------------------------------------------- |
| `--color-background`         | `#fafafa`                  | `#0a0a0a`                   | Window canvas                                                                 |
| `--color-surface`            | `#ffffff`                  | `#0f0f10`                   | Settings groups, cards, dialogs, focused inputs                               |
| `--color-surface-raised`     | `#f4f4f5`                  | `#1c1d1f`                   | Secondary buttons, dropdowns, segmented-control track, toggle off, hover fill |
| `--color-border`             | `#e4e4e7`                  | `rgb(255 255 255 / 8%)`     | Every 1px divider and outline                                                 |
| `--color-text`               | `#27272a`                  | `#f1f3f7`                   | Primary text                                                                  |
| `--color-text-muted`         | `#71717a` (4.63:1)         | `#888b91` (5.80:1)          | Descriptions, group labels, metadata                                          |
| `--color-accent`             | `#2563eb` (4.95:1)         | `#4c8dff` (6.19:1)          | Links, focus rings, selected state, recording meter, logo                     |
| `--color-accent-fill`        | `#1d4ed8`                  | `#2257d6` (3.2:1 vs canvas) | Primary button and toggle-on fill                                             |
| `--color-accent-foreground`  | `#ffffff` (6.70:1 on fill) | `#ffffff` (6.17:1 on fill)  | Text on accent fills                                                          |
| `--color-accent-soft`        | accent at 10%              | accent at 16%               | Selected rows, soft badges                                                    |
| `--color-success`            | `#059669`                  | `#34d399`                   | Success icons and fills                                                       |
| `--color-success-foreground` | `#047857` (5.48:1)         | `#34d399` (10.3:1)          | Success text, e.g. the "Downloaded" badge                                     |
| `--color-warning`            | `#d97706`                  | `#fbbf24`                   | Warning icons and borders (unchanged)                                         |
| `--color-warning-foreground` | `#b45309` (5.02:1)         | `#fbbf24` (11.9:1)          | Warning text. Light `#d97706` is only 3.19:1, so text uses the darker shade   |
| `--color-error`              | `#dc2626`                  | `#f87171`                   | Errors and destructive text (unchanged)                                       |
| `--gradient-signature`       | same in both themes        |                             | `linear-gradient(135deg, #182676 0%, #1f4195 35%, #3269bd 70%, #70b1ff 100%)` |

The logo is a hand-drawn black-ink owl (`src/assets/owl-mark.png`, built by `scripts/owl-icons/build-icons.swift`). It always sits on a white sticker (`--color-logo-paper`) so it stays visible in dark mode, and it is never recoloured with the accent.

The old pink accent (`#da5893`) gave white text 3.62:1, below the 4.5:1 AA minimum. Every pair above passes AA, and the dark fill stays above the 3:1 non-text minimum, so buttons remain visible as shapes on near-black.

## Typography

- System stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`, as T3 Code uses. Owl is a native-feeling desktop app, so it does not ship Inter. (The Paper boards use Inter only because Paper cannot load SF Pro.)
- Mono: `ui-monospace, "SF Mono", Menlo, Consolas, monospace` for shortcuts, paths and logs.
- Scale: 14px row labels and buttons, 13px segmented-control labels and alert text, 12px descriptions, badges and group labels, 11px metadata and key caps. Page titles 18px semibold. Nothing above 24px outside onboarding.
- Weights: 400 body, 500 labels, buttons and badges, 600 headings only.
- Group labels are sentence case ("General"), not uppercase.

## Shape and depth

- Radius: 8px for buttons, inputs and dropdowns; 12px for settings groups, cards and alerts; full pill for segmented controls, toggles, badges and the overlay.
- Depth comes from borders, not shadows. Dark mode has no shadows at all. Light mode may use one soft shadow on floating layers (dropdown menus, dialogs): `0 12px 28px -18px rgb(0 0 0 / 40%)`.
- Filled accent buttons get `inset 0 1px 0 rgb(255 255 255 / 16%)`.
- Hover moves one step up the surface ladder (`surface` → `surface-raised`). Hovers are never tinted with the accent; accent means selection.

## Components

As drawn on the proposed board, in both themes.

- **Primary button:** `accent-fill` background, `accent-foreground` text, inset top highlight, 8px radius, 6px × 14px padding, 14px/500. One per view.
- **Secondary button:** `surface-raised` fill with a `border`. The default button.
- **Ghost button:** transparent, `text` color, `surface-raised` fill on hover.
- **Destructive action:** ghost style with `error` text ("Delete"). No red fills.
- **Segmented control:** pill track in `surface-raised` with a `border` and 3px inner padding. The selected segment is a pill with a `border`, in `surface` (light) or `surface-raised` (dark), with `text`; the others use `text-muted`. Use it for 2–4 mutually exclusive options (theme, activation mode, paste method) instead of a dropdown.
- **Toggle:** 40 × 22px pill. On: `accent-fill` with a white knob. Off: `surface-raised` with a `border`; knob is white in light mode and `text-muted` in dark mode.
- **Input:** 8px radius, 6px × 10px padding. Focused: `surface` background, 1px `accent` border, and a 3px ring of `accent` at 18% (light) or 25% (dark).
- **Dropdown:** `surface-raised` with a `border`, chevron in `text-muted`.
- **Badge:** pill, 3px × 10px, 12px/500. Accent: `accent-soft` with `accent` text. Success: `success` at 10–14% with `success-foreground` text. Neutral: `surface-raised` with `text-muted`.
- **Alert (warning):** 12px radius, `warning` at 8% (light) or 10% (dark) background, `warning` at 20% border, `warning` icon, `warning-foreground` text at 13px.
- **Settings group:** sentence-case label in 12px/500 `text-muted` above a `surface` card with a `border` and 12px radius. Rows are at least 52px tall with a 14px/500 title and a 12px `text-muted` description, separated by `border` hairlines rather than gaps.
- **Focus:** every interactive element shows the `accent` ring described under Input.

## Recording overlay

- Pill on `surface` with a `border`, no shadow in dark mode.
- Cancel on the left, the menu-bar owl on the right, in `text` with its eyes cut out (`scripts/owl-icons/build-icons.swift` writes its layers).
- Recording: an `accent` level meter. The owl is sleepy until the mic is live, then blinks and glances around.
- Transcribing: a `text-muted` label; the owl looks side to side.
- Every motion runs in one loop per screen refresh; with reduced motion the owl stays still.
- No other colors.

## Signature gradient

`linear-gradient(135deg, #182676 0%, #1f4195 35%, #3269bd 70%, #70b1ff 100%)`, sampled from T3 Code's sidebar header. Use it for the sidebar header, with white text. Never on buttons or other controls, and at most once per screen.

## Migration

Steps 1–3 are implemented. Each step is a separate, reviewable change. Steps 1 and 2 change values and add tokens without renaming the ~100 existing `*-logo-primary` classes, which kept Owl easy to merge with Handy back when it still did.

**Step 1: tokens (`src/styles/theme.css`).**

- Repoint existing tokens: `--color-logo-primary` → `accent` (`#2563eb` / `#4c8dff`); `--color-background-ui` → `accent-fill` (`#1d4ed8` / `#2257d6`), which gives it a light/dark pair for the first time; `--color-logo-stroke` → `text` values (replacing `#382731` / `#fad1ed`); `--color-background` → `#fafafa` / `#0a0a0a`; `--color-text` → `#27272a` / `#f1f3f7`.
- Add the new tokens from the table that have no equivalent today: `surface`, `surface-raised`, `border`, `text-muted`, `accent-fill`, `accent-soft`, `success`, `success-foreground`, `warning-foreground`, and the signature gradient. Register them in the `@theme inline` block in `App.css`.
- Mix the log console surface toward `--color-text` so it stays visible on near-black and follows the `data-theme` override.

**Step 2: remove the remaining pink.**

- Hardcoded `#FAA2CA` in `ui/AudioPlayer.tsx`, and the defaults in the unused `CancelIcon`, `MicrophoneIcon` and `TranscriptionIcon`.
- The overlay's warm dark grays in `RecordingOverlay.css` (`#a3a09a`, `#6f6c66`) become neutral.
- Tray icon and app icon assets in `src-tauri/`.

**Step 3: components adopt the tokens.** Button (inset highlight, destructive as ghost), toggle size and off state, input focus ring, settings-group label case and card surface, Alert's status colors, and a new segmented control for the theme selector.

**Step 4 (optional): rename** to the semantic token names, now that Owl no longer merges Handy.

| Current                                                               | Usages | Becomes                                             |
| --------------------------------------------------------------------- | ------ | --------------------------------------------------- |
| `logo-primary` used as UI accent (`bg-`, `border-`, `text-`, `ring-`) | ~100   | `accent` / `accent-soft`                            |
| `background-ui`                                                       | ~14    | `accent-fill`                                       |
| `mid-gray/10`, `mid-gray/20` fills                                    | ~45    | `surface-raised`                                    |
| `mid-gray/20`, `/40`, `/80` borders                                   | ~50    | `border`                                            |
| `text-mid-gray`                                                       | ~28    | `text-muted`                                        |
| `red-*`, `yellow-*`, `green-*` ad hoc                                 | few    | `error`, `warning`, `success` and their foregrounds |

## Rules

- No pink.
- No new hex values in components. Add a token to `theme.css` or use an existing one.
- No raw Tailwind palette colors (`blue-500`, `gray-400`). Use the semantic tokens.
- Both themes and the `data-theme` override must work for every change.
- The signature gradient appears at most once per screen.
- New user-facing strings go through i18n (see `AGENTS.md`).
- When a decision here changes, update this file in the same change. The maintainer keeps the Paper boards in step.
