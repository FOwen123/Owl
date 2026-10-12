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
- [ ] Start/stop from the tray and a shortcut (the menu bar's Notetaker menu is done)
- [x] Wispr Flow-style notes: own prompt from 25 Wispr summaries, written automatically when Post Process is on (`managers/meeting.rs`)
- Speaker labels, calendar and the rest of the meeting work moved to Phase 4

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

- [x] **Comparison logic** (pure function, tests first): `src-tauri/src/correction.rs`, `suggest(pasted, edited) -> Vec<Replacement>`. Three steps:
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
- [x] **Text-box watcher** (macOS): `src-tauri/src/correction_watch.rs`
  - Start it after a successful paste (the paste in `actions.rs`)
  - Read the focused text box's value through the Accessibility API
  - Check every 0.5 s for 45 s; compare once the text has been still for 1.5 s
  - Stop if I switch apps; never read password fields (`AXSecureTextField`)
  - Apps that don't expose their text (some terminals and Electron apps) are skipped silently
- [x] **Settings** (`settings.rs`, with tests):
  - `learn_from_edits` toggle, default on
  - `edit_suggestions` waiting for Add/Dismiss; `dismissed_suggestions`, so a dismissed fix is never offered again
  - Commands in `src-tauri/src/commands/edits.rs`
- [x] **UI:**
  - Wispr Flow-style card in the recording overlay (`src/overlay/`, `overlay.rs`): "Adding to dictionary · Owel → Owen", an X, and a 5-second countdown that runs around the card's edge. No X → the fix is added with a short "Added" animation. Waits while a dictation is on screen; with the overlay turned off, fixes wait on the Dictionary page instead.
  - A Suggestions list at the top of `src/components/owl/DictionaryPage.tsx` with Add / Dismiss; Add appends to `replacements`
  - "Learn from my edits" switch under Replacements
  - Strings in `en`, `zh`, `zh-TW`
- [ ] **Real test** in a built `Owl.app` (Accessibility only works there):
  - Dictate into Notes and TextEdit
  - Fix one English word and one Chinese word
  - Check the card appears, X skips the fix, waiting adds it, and it works on the next dictation
- [ ] Words work with every model (decided): passed as vocabulary hints to any model that supports them (transcribe.cpp `vocabulary`, `Feature::Vocabulary`: Whisper, Qwen3-ASR, FunASR-Nano, Granite, MOSS), replacing the Whisper-only prompt; for the rest (Nemotron 3.5) through the fix-up, which never rewrites a word the OS spell checker knows ("cloud" stays with "Claude" in Words)
- [ ] **Bug:** dictated "just keep it simple", Owl wrote "just skip it simple"; fixing skip → keep offered a Dictionary entry "skip → keep". Two ordinary words count as "misheard" because their spelling similarity (0.667) passes the 0.64 bar. Spelling similarity can't separate "send → sent" (0.883, a rewrite) from "Owel → Owen" (0.883, a mishearing). Fix (decided): ask the OS spell checker (macOS `NSSpellChecker`, Windows Spell Checking API; built in, nothing to download). Misheard word is a real word ("skip") → no replacement rule, since it would rewrite that word everywhere. Misheard word isn't a word ("Owel") → offer the replacement. Fixed word is a name or term (capitals, acronym, or unknown to the spell checker, like "Depick") → also add it to Words. Chinese keeps its same-sound rule. Tests: skip→keep, send→sent, ship→shop (none), Owel→Owen, depict→depick (Words only)
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
  - Handy's contribution process: `.github/ISSUE_TEMPLATE/`, `.github/PULL_REQUEST_TEMPLATE.md`, `CONTRIBUTING.md`, `CONTRIBUTING_TRANSLATIONS.md`, and the "GitHub workflow" section of `AGENTS.md`. They describe Handy's feature freeze and Discussions. Delete now; write short Owl versions in Phase 6 if needed.
- [x] **Keep:** Handy-related but needed
  - `README.handy.md`, `LICENSE`: the MIT licence requires keeping Handy's copyright and credit
  - Internal file names (`handy.png`, `handy_warning.png`, the `handy` crate): they already hold Owl art, and renaming makes upstream merges harder (renaming rule)
  - Model downloads from `blob.handy.computer` (`src-tauri/src/catalog/catalog.json`, `managers/model.rs`): still needed until Phase 6 re-hosts them
  - `scripts/gen_catalog.py` (builds `catalog.json`), `scripts/mirror_models.py` (useful for the Phase 6 re-host), `scripts/ci/`
  - `src/components/update-checker/` (see AGENTS.md)
  - Windows/Linux code and config (`src-tauri/nsis/`, `tauri.windows.conf.json`, platform branches in `src/`): `build.rs` refers to them, and deleting them causes merge conflicts for no gain on macOS
  - `BUILD.md`: real build steps; trim it to macOS later
  - `src-tauri/icons/` Android, iOS and Windows Store sizes: these are Owl icons, written again by every `tauri icon` run
- [x] **Check after cleanup:** `bun install` (no postinstall error), `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cargo test`, and `git grep -i handy.computer` shows only the model download URLs

## Phase 3c: Cleanup rules and levels ✅

None / Light / Medium, checks on the AI's reply, Chinese and English rules, silence phrases, voice commands (`cleanup_rules.rs`, `cleanup_guard.rs`). The plan file was removed when done; see git history for `TODO-CLEANUP.md`.

- [ ] Later: "press enter" at the end of a dictation sends Return after pasting (Felix `apply_voice_triggers` in `scratchpad.rs`)

## Phase 4: Full meeting pipeline

What each item does and when it's done: [docs/prd-meeting-pipeline.md](docs/prd-meeting-pipeline.md). Build order: merge Handy → dictionary fixes → speaker labels → notes → live transcript and echo → import/export → outside access → edit by voice and auto dictionary, then one pull request; Nemotron 3 after that in its own worktree.

Like Glimpse and Muesli, but local-first and in Owl's shape. **Platform rule:** Mac first, but pick parts that also run on Windows (transcribe.cpp or ONNX, not CoreML/FluidAudio); keep Mac-only code in small files. **Open** = how to build it hasn't been decided yet.

**Speaker labels** (built on branch `feat/meeting-pipeline`, worktree `../Owl-speakers`)

- [x] Merge Handy's latest `main` first (decided: merge, not rebuild; 10 commits since 2026-10-05): transcribe.cpp 0.4.0 (#2265; Owl has 16 compile errors on 0.4.0 without it), transcription in its own worker (#2208), model-loading status (#2227), mic capture moved to the `handy-recorder` crate (#2188), transcribe.cpp's own VAD (#2267). Trial merge: 33 conflicting files, 26 of them translations and 5 docs Owl changed; the real work is `audio_toolkit/audio/mod.rs`, `lib.rs`, and Owl's meeting recorder, which uses `cpal`, `rtrb` and `FrameResampler` that Handy removed. The meeting mic moves to `handy-recorder` (decided: reports lost audio and unplugged mics, gives 30 ms pieces for speech detection); system capture keeps `rtrb`
- [x] Meetings use transcribe.cpp's speech detection (Silero) instead of the loudness check, so quiet speakers aren't dropped (decided)
- [x] Engine first: Sortformer v2.1 on upstream transcribe.cpp 0.4.0 (4 speakers, 80% agreement on the 12 Oct meeting, 1 speaker correctly found on the call track), no fork (decided: build on it now, swap in Nemotron 3 later). Built: it runs in its own process (`owl --owl-diarize`), since transcribe.cpp's engine worker only opens speech sessions
- [x] Split only the call-audio track into Speaker 1..N; the mic stays "You". Cut the audio where the speaker changes, so each line has one speaker
- [x] Store a speaker number per line (old meetings still open); click a speaker to rename them for the whole meeting. The summary uses placeholders (`<@speaker:2>`, like Wispr Flow) that Owl fills in, so a rename updates it without a new AI call
- [x] Speaker model download: one "Download speaker labels" button on the Notetaker page, no model picker. Sortformer v2.1 (139 MB) comes from the existing `handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf` repo; the Nemotron 3 file Owl makes later goes on `owl-dictation`. Listed like Glimpse does: full `huggingface.co/<org>/<repo>/resolve/<revision>/<file>` URL, size and SHA-256
- [ ] Later: the AI suggests names from the conversation ("thanks, Harry") and macOS Calendar attendees; I confirm
- Not doing: remembering voices across meetings (privacy)

**Nemotron 3 speaker model** (later, own worktree)

- [ ] Nemotron 3 Diarization (8 speakers). Upstream status (2026-10-12): the maintainer's own port is open PR #175 (`handy-computer:nemotron3-diar`, +8,282 lines, model type `nemotron3_diar`, "stage 1-4", untouched since 2026-09-26, now conflicting with the 2026-10-05 diarization rework); issue #170 says "will come in #175". No model file published; its converter needs NVIDIA's NeMo toolkit. Glimpse's port (`glimpse-hq/transcribe.cpp`) works today but its model file doesn't load in #175 (missing `output_hop`), so the two have diverged. NVIDIA's own GGUF loads in neither. Path (decided): finish the maintainer's PR #175 ourselves on 0.4.0, in its own worktree (`../Owl-nemotron3`), after the speaker feature works on Sortformer v2.1:
  - Ask on #175 whether he's still on it and offer help (public comment from my GitHub: ask me first)
  - Rebase onto 0.4.0, finish the remaining porting stages (accuracy checks)
  - Make the model file once with his converter (needs NVIDIA's NeMo toolkit), host it on `owl-dictation` on Hugging Face
  - Offer the fixes back to #175; switch Owl to the official version when it merges
  - Check: the 12 Oct meeting (Glimpse's build: 2 speakers, 88% agreement with Wispr Flow, 88 min in 6 s)

**Notes**

- [x] Auto titles: the AI writes the title in the same call as the summary, saved in a new `title` column and shown in the meeting list. Without an AI the title stays the date. Titles can be edited by hand
- [x] Meeting page layout (decided): list rows show title, date, duration and the summary's first line; Import audio and Start meeting at the top. The meeting page has a back link, Export menu (Markdown, text, SRT, VTT), and a ⋯ menu (Re-transcribe, Delete); an editable title; date, duration and speaker chips (click to rename); Notes with the template dropdown and Summarize again; the transcript with Copy at the bottom. While recording, the live transcript fills the page with Stop
- [x] Templates: built-ins General (today's notes), 1:1, Interview, Stand-up, Lecture, plus my own, edited on the Notetaker page. Pick one from a dropdown next to "Summarize again"; a default in Notetaker settings. Keep the UI clean
- [x] After building the meeting pages, export the design to Paper, so later changes are made in Paper first and then implemented
- [x] Re-summarize with another template or AI model (the "Summarize again" button gets a picker)
- [x] Re-transcribe a saved meeting from its kept audio, so new dictionary words and a better model apply (Muesli has this)

**Echo removal** (only matters without earphones)

- [ ] Test on a real call: record one on laptop speakers, count "You" lines that repeat the other side (only the synthetic test has been run)
- [x] Remove the speakers' sound from the mic with **LocalVQE v1.4-AEC** (decided: echo only, 3 MB, Apache-2.0, model from `LocalAI-io/LocalVQE`; Muesli uses it), with the call audio as the reference. Also test v1.2 (echo + noise, Muesli's choice) on the same recording. It ships its own copy of ggml, so build it as a separate library to avoid clashing with transcribe.cpp's. Bundled inside the app (decided). Runs live, before each stretch of speech is transcribed, since live lines are final (decided); the raw `mic.wav` is kept so Re-transcribe can redo it. Built and tested on a synthetic call (v1.4: 65 dB echo removed, v1.2: 63 dB; the cleaned mic transcribed exactly). Built CPU-only by `scripts/build_localvqe.sh`

**Live transcript**

- [x] Show the transcript while the meeting runs (decided): each stretch of speech is transcribed as soon as it ends, with the local model, and that text is final. After the meeting Owl only adds speaker labels and joins neighbouring lines from the same speaker. Wispr Flow instead shows Soniox's live lines and replaces them with a second, cloud pass; Owl would run the same model twice. The manual "Re-transcribe" covers switching to a better model

**Import and export**

- [x] Import audio or video into the meeting list: one track, so all of it goes through speaker labels. Decoder: `symphonia` (decided; pure Rust, Mac and Windows, already in Owl through `rodio`): mp3, m4a/AAC, wav, flac, ogg/Vorbis, and the audio inside mp4/mov. It can't decode Opus (WebM, `.opus`, WhatsApp voice notes), so say so plainly for those files
- [x] Export a meeting as Markdown, plain text, SRT or VTT subtitles (decided: plain text formatting, no library). Subtitles need each line's end time, so store `end_ms` too. PDF later

**Outside access**

- [x] CLI: `owl meetings`, `owl meeting <id> --notes|--transcript`, `owl transcribe <file>` (Handy already has CLI flags in `cli.rs`). Install from Advanced → Other apps
- [x] Local API: an OpenAI-compatible `/v1/audio/transcriptions`, off by default, only reachable from this Mac, needs a token. Port 8723, token in Advanced → Other apps
- [x] AI-agent access: `owl mcp`, a read-only MCP server so Claude Code and others can search and read meetings and dictations (Glimpse has this); no starting or deleting. Tested with Claude Code: `claude mcp add owl -- owl mcp`

**Calendar** (later, after the rest of this phase)

- [ ] Read macOS Calendar (EventKit), find Zoom/Meet/Teams links, offer "Join & record" (reference: Muesli `CalendarMonitor.swift`, `MeetingCandidateResolver.swift`)

## Phase 5: Dictation extras

- [x] Edit by voice, like Wispr Flow's Command Mode (decided): its own shortcut, Option+Shift+Space (decided; Wispr's default is Fn + Ctrl). That key was Handy's "Transcribe with Post-Processing"; that action stays in Settings with no key assigned (decided). Select text, hold the shortcut, say what to change ("make this shorter", 「翻成英文」), release: the Post Process AI's answer replaces the selection. Esc cancels. With nothing selected, the answer is written at the cursor. Undo is the app's own Cmd+Z. Built: Owl had already hidden Handy's post-processing key (Post Process runs on the normal shortcut), so it keeps no key and stays hidden. The selection is read through Accessibility, with Cmd+C for apps that hide it
- [x] Auto dictionary, like Glimpse's and Wispr Flow's: when I correct a pasted word into a name, brand or acronym, add that word to the Dictionary's **Words** list (decided), as well as the replacement pair Phase 3b learns, so the model spells it right however it mishears it next time. Asks with the existing card (decided). Note: Nemotron 3.5 takes no vocabulary hints (only Whisper, Qwen3-ASR, FunASR-Nano, Granite and MOSS do), so for it Words work through the fix-up after transcription and the summary's spelling list. Built, including capital-only fixes ("depick" → "Depick")

## Phase 6: Before open-sourcing (last)

- [ ] Own model hosting: use the `owl-dictation` Hugging Face organization for models Owl converts or must host itself; use existing verified repositories for other downloads. Skip Cloudflare R2 for now. The public collection is for browsing; the bundled catalog controls the app.
- [x] Replace 15 legacy download URLs with revision-pinned, checksum-matched Hugging Face sources; retain filenames and engines. Remove all GigaAM entries and backend support, and exclude GigaAM from catalog regeneration. No model copies uploaded.
- [ ] Update `AGENTS.md` "Target: macOS only" to the Phase 4 platform rule
- [ ] Copy `breeze-asr-q5_k.bin` to `owl-dictation`: it no longer depends on Handy's server, but now comes from one person's Hugging Face repo (`alan314159/Breeze-ASR-25-whispercpp`)
- [ ] Check the `vad-rs` license (cjpais copy has none; check `thewh1teagle/vad-rs`)
- [ ] Scan git history for secrets
- [ ] Own release feed: generate an updater key (`bun run tauri signer generate`), set `plugins.updater` to my GitHub releases, restore the `HANDY_DISABLE_UPDATER` check in `settings.rs`
- [ ] Optional: Silero VAD v4 → v6.2 (needs `vad-rs` changes for v5+'s `state` tensor; see closed Handy PR #1790). Re-check after Phase 4 step 1: Handy's merge brings transcribe.cpp's own Silero VAD (#2267), which may replace `vad-rs` and settle this and the license item above
- [ ] Update `README.md`, screenshots, and make the repo public
