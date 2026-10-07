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
- [ ] Optional: speaker labels within "Others" (reference: Felix `meetings/diarize.rs`)

## Phase 3: Design and polish ✅ (core)

- [x] Tokens per `DESIGN.md` in `src/styles/theme.css`, registered in `src/App.css`; system font; no pink
- [x] Components: Button, Toggle, Input, Textarea, Dropdown, Select, Slider, Dialog, Alert, Badge, settings rows/groups, new `SegmentedControl` (theme picker)
- [x] Sidebar: gradient header, Dictation / Notetaker / Dictionary / Snippets / History on top, settings pages at the bottom
- [x] Recording pill: tokens from `theme.css`, transcribing bar in the signature gradient
- [ ] Home page with recent dictations, like Wispr Flow's: `src/App.tsx` + new `src/components/home/`
- [ ] Onboarding "Welcome to Owl" with the signature gradient: `src/components/onboarding/`
- [ ] Remaining raw palette colors in debug, onboarding and model-selector components (DESIGN.md migration step 4)
- [ ] Later ideas: Insights (word count, WPM), per-app Style, Transforms on selected text

## Phase 4: Before open-sourcing (later)

- [ ] Re-host the 16 legacy models on my Hugging Face (Breeze ASR first; check each model's license); update URLs in `src-tauri/src/managers/model.rs`, keeping the SHA-256 values
- [ ] Back up `breeze-asr-q5_k.bin` now in case Handy's link disappears
- [ ] Check the `vad-rs` license (cjpais copy has none; check `thewh1teagle/vad-rs`)
- [ ] Scan git history for secrets
- [ ] Own release feed: generate an updater key (`bun run tauri signer generate`), set `plugins.updater` to my GitHub releases, restore the `HANDY_DISABLE_UPDATER` check in `settings.rs`
- [ ] Optional: Silero VAD v4 → v6.2 (needs `vad-rs` changes for v5+'s `state` tensor; see closed Handy PR #1790)
- [ ] Update `README.md`, screenshots, and make the repo public
