# Owl TODO

Owl is a personal fork of [Handy](https://github.com/cjpais/Handy), with English and Chinese (Traditional/Simplified) dictation, a Wispr Flow-style notetaker, and its own design.
Target: macOS on Apple Silicon. Upstream remote: `upstream` → cjpais/Handy.

**Renaming rule:** rename only what users see. Keep internal names (`handy` crate, `handy_keys`, `HANDY_*` env vars) so `git merge upstream/main` stays clean.

**Check before calling a task done:** `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cd src-tauri && cargo test --lib && cargo clippy`, then try it in `bun run tauri dev` (or the UI alone at `http://localhost:1420/?mock` after `bun run dev`).

---

## Phase 0: Setup and rebrand ✅

- [x] Toolchain: Rust, bun, cmake, `bun install`, VAD model, first `bun run tauri dev`
- [x] GitHub: all upstream workflows disabled on `FOwen123/Owl` (files kept)
- [ ] Later: add one small macOS-only check workflow (lint + `cargo test`)
- [x] `tauri.conf.json`: `identifier` `com.hao.owl`, `productName` `Owl`, `version` `0.1.0`, updater endpoints emptied
- [x] Updater forced off: `update_checks_forced_disabled()` in `src-tauri/src/settings.rs`; removed from the footer
- [x] UI strings "Handy" → "Owl" in all 26 locales; window title; `index.html` title
- [x] Logo: `src/components/icons/OwlLogo.tsx` (replaces `HandyTextLogo` / `HandyHand`)
- [x] App, tray and overlay icons from `scripts/owl-icons/`
- [x] About page: donate link removed, "Built on Handy" credit, source link → Owl repo
- [x] Docs: `README.md` (Owl), `README.handy.md` (original), `LICENSE` (your line added), `AGENTS.md` (Owl section)

## Phase 1: Make it usable ✅

- [x] Quick language switch in the tray menu: Auto / English / 中文（繁體）/ 中文（简体）(`dictation_language` in `settings.rs`, `tray.rs`, handler in `lib.rs`)
- [ ] Try real dictation in English, Traditional and Simplified with Breeze ASR and Whisper Turbo
  - Note: Traditional uses OpenCC `s2tw`, which writes 台 as 臺. Add a Dictionary replacement (臺 → 台) if you prefer 台.
- [x] Snippets: `src-tauri/src/snippets.rs` (matching + tests), applied in `process_transcription_output` (`actions.rs`), skips AI cleanup; page `src/components/owl/SnippetsPage.tsx`
- [x] Dictionary: `src-tauri/src/dictionary.rs` (replacements + tests), applied after Chinese script conversion in `post_process_transcription_text` (`transcription.rs`); page `src/components/owl/DictionaryPage.tsx` (custom words moved here from Advanced)
- [ ] Optional: "Add to dictionary" button on history items (`src/components/settings/history/HistorySettings.tsx`)

## Phase 2: Notetaker ✅ (needs one real-meeting test)

- [x] System audio: `src-tauri/src/audio_toolkit/audio/system_capture.rs` (Core Audio process tap, ported from Felix); `NSAudioCaptureUsageDescription` in `Info.plist`
- [x] Recorder, chunked transcription, storage (`meetings.db`, separate from Handy's history): `src-tauri/src/managers/meeting.rs`
- [x] Lines labelled You (mic) / Others (system audio); no diarization
- [x] Summary with the Post Process AI provider: `src-tauri/src/commands/meetings.rs`
- [x] Page: `src/components/owl/NotetakerPage.tsx` (start/stop, list, transcript, summary, copy, delete)
- [ ] Real test in a built `Owl.app`: macOS only grants System Audio Recording to a bundled app, so system audio can't be verified from `tauri dev`. Mic recording is verified.
- [ ] Start/stop from the tray and a shortcut
- [ ] Optional: speaker labels within "Others" (reference: Felix `meetings/diarize.rs`; Muesli uses pyannote models through FluidAudio, which is Swift-only, so check sherpa-onnx for Rust)
- [ ] Optional: calendar meeting detection: read macOS Calendar (EventKit), find Zoom/Meet/Teams links, offer "Join & record" (reference: Muesli `CalendarMonitor.swift`, `MeetingCandidateResolver.swift`)

## Phase 3: Design and polish ✅ (core)

- [x] Tokens per `DESIGN.md` in `src/styles/theme.css`, registered in `src/App.css`; system font; no pink
- [x] Components: Button, Toggle, Input, Textarea, Dropdown, Select, Slider, Dialog, Alert, Badge, settings rows/groups, new `SegmentedControl` (theme picker)
- [x] Sidebar: gradient header, Dictation / Notetaker / Dictionary / Snippets / History on top, settings pages at the bottom
- [x] Recording pill: tokens from `theme.css`, transcribing bar in the signature gradient
- [ ] Home page with recent dictations, like Wispr Flow's: `src/App.tsx` + new `src/components/home/`
- [ ] Onboarding "Welcome to Owl" with the signature gradient: `src/components/onboarding/`
- [ ] Remaining raw palette colors in debug, onboarding and model-selector components (DESIGN.md migration step 4)
- [ ] Later ideas: Insights (word count, WPM), per-app Style, Transforms on selected text

## Phase 3b: Learn from my edits ("active learning")

After Owl pastes, it watches the text box for a short time. When I fix a misheard word, it suggests a Dictionary replacement. Nothing is retrained and no backend is needed. Ported from Muesli (`DictationCorrectionMonitor.swift`, MIT; credit it in the file header), plus Chinese rules Muesli doesn't have.

**Design:** one implementation for both languages, on one branch (`feat/learn-from-edits`, merged to `main` after the real test). Don't reuse Handy's fuzzy matcher (`audio_toolkit/text.rs`): it does a different job (finding known words in a transcript), and its Soundex check is English-only.

- [ ] **Comparison logic** (pure function, tests first): `src-tauri/src/correction.rs`, `suggest(pasted, edited) -> Vec<Replacement>`. Three steps:
  1. **Split and line up:** split into tokens (an English word, or one Chinese character), line up both texts with a longest-common-subsequence diff (~30 lines, no new crate), and collect the changed runs. This also handles mixed text like "我用 cloud code 寫的".
  2. **Judge each changed run by its script:** English rules or Chinese rules below.
  3. **Shared filters:** below.
  - Shared rules:
    - Only compare when most of the text still matches (otherwise it's a different message)
    - Skip texts over 512 words
    - At most 2 words (or 4 Chinese characters) per suggestion
    - Ignore punctuation-only and full/half-width changes
  - English rules (Muesli's), using `strsim::jaro_winkler` (already a dependency):
    - Spelling must be similar (≥ 0.64): "Owel → Owen" ✅, "meeting → call" ❌
    - Ignore common words ("the → a")
    - Capitals count only when they signal a name or brand ("iphone → iPhone", "openai → OpenAI")
    - Acronyms: "a p i → API"
    - Joined words: "chat GPT → ChatGPT"
    - Number shorthand: "internationalization → i18n"
    - Two real words with one a prefix of the other is a grammar edit: "test → testing" ❌
  - Chinese rules (new):
    - Compare character by character, since there are no spaces
    - Suggest when the changed run is 1–4 characters with matching text on both sides
    - Same pronunciation, ignoring tones, means a misheard character: "公事 → 公式" ✅. Different pronunciation means a rewrite: "我們 → 他們" ❌. Needs a pinyin crate; check that it covers Traditional characters.
    - Near-sounds count too ("fuzzy pinyin", 模糊音), because Taiwanese Mandarin and speech models blur them: zh/z, ch/c, sh/s, n/l, ing/in, eng/en. So "是 (shi) → 四 (si)" counts as misheard. This is a small lookup table.
    - Script variants count as a preference: "臺北 → 台北" ✅ (check with OpenCC from `chinese_script.rs`)
    - Never suggest a single-character replacement: a Dictionary entry "在 → 再" would change every 在. Widen to 2+ characters using the neighbouring character.
    - Ignore number-style changes ("二十 → 20"): that's formatting, not a dictionary word
  - English words inside Chinese text ("我用 cloud code") use the English rules
- [ ] **Text-box watcher** (macOS): `src-tauri/src/correction_watch.rs`
  - Start it after a successful paste (`paste()` in `clipboard.rs`)
  - Read the focused text box's value through the Accessibility API
  - Check every 0.5 s for 45 s; compare once the text has been still for 1.5 s
  - Stop if I switch apps; never read password fields (`AXSecureTextField`)
  - Apps that don't expose their text (some terminals and Electron apps) are skipped silently
- [ ] **Settings** (`settings.rs`, with tests):
  - `learn_from_edits` toggle, default on
  - `dismissed_suggestions`, so a dismissed fix is never offered again
- [ ] **UI:**
  - A small "Add Owel → Owen to Dictionary?" prompt after a suggestion
  - A Suggestions list at the top of `src/components/owl/DictionaryPage.tsx` with Add / Dismiss; Add appends to `replacements`
  - Strings in `en`, `zh`, `zh-TW`
- [ ] **Real test** in a built `Owl.app` (Accessibility only works there):
  - Dictate into Notes and TextEdit
  - Fix one English word and one Chinese word
  - Check the suggestion appears and works on the next dictation
- [ ] **Later:** save accepted fixes and raw → cleaned pairs as training data for my own cleanup model (fine-tune Qwen 3.5 0.8B/2B with LoRA on MLX, English + Traditional Chinese)

### Clean out Handy-only files

Audit done 2026-10-08. One commit, on its own, separate from the edit-learning work. Merge cost: if Handy later edits a file we deleted, `git merge upstream/main` stops on it; resolve with `git rm <file>`.

- [x] **Delete:** Handy's, not used by Owl
  - `sponsor-images/` (Handy's sponsors) and the "Sponsors" section of `README.handy.md` that shows them
  - `.github/FUNDING.yml`: it puts a "Sponsor" button for cjpais on my repo
  - `CRUSH.md`: Handy's notes for the Crush AI tool; `AGENTS.md` covers this
  - Nix/Linux packaging: `flake.nix`, `flake.lock`, `.nix/`, `nix/`, `scripts/check-nix-deps.ts`, `.github/workflows/nix-check.yml`. Also remove `"postinstall": "bun scripts/check-nix-deps.ts"` from `package.json`, which runs on every `bun install`.
  - `docs/troubleshooting/ubuntu-26-04-gnome-wayland` (Linux)
  - Handy's 9 workflows in `.github/workflows/` (already disabled; they use Handy's signing secrets). Phase 0 has the to-do for one small Owl workflow.
- [x] **Replace:** Handy content that Owl shows or links to
  - What's New: Handy's 0.9.0 and 0.9.7 notes (`src/content/release-notes/`, `public/release-notes/`). Delete the notes, keep the mechanism (`src/components/whats-new/`) for Owl's own notes later.
  - `src/components/SecureInputWarning.tsx` links to `handy.computer/docs/troubleshooting`. Point it at a troubleshooting section in Owl's `README.md`.
  - `tests/app.spec.ts`: rename "Handy App" to "Owl"
  - Handy's contribution process: `.github/ISSUE_TEMPLATE/`, `.github/PULL_REQUEST_TEMPLATE.md`, `CONTRIBUTING.md`, `CONTRIBUTING_TRANSLATIONS.md`, and the "GitHub workflow" section of `AGENTS.md`. They describe Handy's feature freeze and Discussions. Delete now; write short Owl versions in Phase 4 if needed.
- [x] **Keep:** Handy-related but needed
  - `README.handy.md`, `LICENSE`: the MIT licence requires keeping Handy's copyright and credit
  - Internal file names (`handy.png`, `handy_warning.png`, the `handy` crate): they already hold Owl art, and renaming makes upstream merges harder (renaming rule)
  - Model downloads from `blob.handy.computer` (`src-tauri/src/catalog/catalog.json`, `managers/model.rs`): still needed until Phase 4 re-hosts them
  - `scripts/gen_catalog.py` (builds `catalog.json`), `scripts/mirror_models.py` (useful for the Phase 4 re-host), `scripts/ci/`
  - `src/components/update-checker/` (see AGENTS.md)
  - Windows/Linux code and config (`src-tauri/nsis/`, `tauri.windows.conf.json`, platform branches in `src/`): `build.rs` refers to them, and deleting them causes merge conflicts for no gain on macOS
  - `BUILD.md`: real build steps; trim it to macOS later
  - `src-tauri/icons/` Android, iOS and Windows Store sizes: these are Owl icons, written again by every `tauri icon` run
- [x] **Check after cleanup:** `bun install` (no postinstall error), `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cargo test`, and `git grep -i handy.computer` shows only the model download URLs

## Phase 4: Before open-sourcing (later)

- [ ] Re-host the 16 legacy models on my Hugging Face (Breeze ASR first; check each model's license); update URLs in `src-tauri/src/managers/model.rs`, keeping the SHA-256 values
- [ ] Back up `breeze-asr-q5_k.bin` now in case Handy's link disappears
- [ ] Check the `vad-rs` license (cjpais copy has none; check `thewh1teagle/vad-rs`)
- [ ] Scan git history for secrets
- [ ] Own release feed: generate an updater key (`bun run tauri signer generate`), set `plugins.updater` to my GitHub releases, restore the `HANDY_DISABLE_UPDATER` check in `settings.rs`
- [ ] Optional: Silero VAD v4 → v6.2 (needs `vad-rs` changes for v5+'s `state` tensor; see closed Handy PR #1790)
- [ ] Update `README.md`, screenshots, and make the repo public
