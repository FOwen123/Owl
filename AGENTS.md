# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## Owl

This repo is **Owl**. It started as a fork of [Handy](https://github.com/cjpais/Handy) and now stands on its own: it no longer merges Handy, and its code uses Owl's names. Handy's MIT notice stays in `LICENSE`, and the README and About page credit it.

- **Names:** the crate is `owl`, the library `owl_app_lib`, environment variables `OWL_*`. "handy" left in the code names something outside Owl: the `handy-keys` and `handy-recorder` libraries (the keyboard setting value `handy_keys` is stored in users' settings), and the `handy-computer` Hugging Face org and `blob.handy.computer`, where models download from.
- **Windows and Linux:** the code stays so Owl can ship there later, but it's untested and unsigned. Signing is added with Owl's own credentials when it ships (`tauri.conf.json` has no Windows signing).
- **App ID:** `com.hao.owl`. Data lives in `~/Library/Application Support/com.hao.owl/`.
- **Updater:** on. It reads `latest.json` from Owl's newest published GitHub release, so drafts never reach users; `OWL_DISABLE_UPDATER=1` turns it off. Tag builds sign the update bundles with the `TAURI_SIGNING_PRIVATE_KEY` secret (its public half is in `tauri.conf.json`); other builds, local ones included, skip them and need no key. Never point it at Handy's releases.
- **Design:** follow `DESIGN.md` (tokens in `src/styles/theme.css`, no pink, one accent).
- **Plan:** the maintainer keeps a local `TODO.md`, out of git. Owl's own features live in: `src-tauri/src/snippets.rs`, `src-tauri/src/dictionary.rs`, `src-tauri/src/managers/meeting.rs` (+ `commands/meetings.rs`, `audio_toolkit/audio/system_capture.rs`), and their pages in `src/components/owl/`.
- **UI without the backend:** `bun run dev`, then open `http://localhost:1420/?mock` (`src/dev/mockBackend.ts`, dev builds only). Add fake responses there when a page needs them. The overlay is at `/src/overlay/index.html?mock` (add `&state=transcribing` or `&state=notice`).
- **README screenshots:** `docs/screenshots/` are taken from `?mock` at 1100×720, 2× scale, dark theme, so they never show real data, then framed with `scripts/screenshots/frame.sh IN OUT` (Chrome). Retake them when a page they show changes.
- **README:** it documents the build requirements and every user-facing feature. Check a feature in the code before the README mentions it, and update it when a feature, default shortcut, permission or requirement changes. `README.zh-CN.md` (Simplified) and `README.zh-TW.md` (Traditional) are its Chinese copies; change all three together.
- **Translations:** every new key goes into all locales (`bun run check:translations` fails otherwise). Write real `en`, `zh` and `zh-TW` strings; other locales get the English text.
- **Logo and icons:** one hand-drawn source, `scripts/owl-icons/owl-source.png`. `swift scripts/owl-icons/build-icons.swift` writes the in-app mark (`src/assets/owl-mark.png`), the menu-bar icons (`src-tauri/resources/*.png`) and `/tmp/owl-app-icon.png`; then run `bun run tauri icon /tmp/owl-app-icon.png`.
- **Dictation eval:** `owl --eval DIR [--post-process] [--eval-name NAME]` replays a test set (`set.jsonl` + audio) and scores hearing, final edits, Words, numbers and Traditional/Simplified script (`eval.rs`). Sets: a Wispr Flow history export (`scripts/eval/build_set.py`), public zh-TW / mixed / English recordings (`scripts/eval/fetch_public.py`), and Owl's own learning log (`owl --export-learning DIR`). Compare runs with `scripts/eval/compare.py DIR OLD NEW`. `scripts/eval/label.py` opens a page for hand-labeling a Wispr set (what was said, what was wanted) into `labels.json`, which `--eval` then scores against instead of Wispr's text. Keep clips, keys and results outside the repo and never commit them. Test against a portable copy (a `portable` file next to the binary) so the real app data isn't touched.
- **Learning:** opt-in log of dictations and the user's fixes (`learning.rs`); `learner.rs` proposes changes (Words, swaps, hint order and cap, a context passage for the speech model, cleanup examples), keeps only those that score better on the newest fixed dictations, and lists them with Undo on the Learning page. `owl --learn` runs it once.
- **Sign in with ChatGPT:** `chatgpt.rs`, OpenAI's open-source flow (loopback `127.0.0.1`, PKCE, self-registration as "Owl"); `llm_client.rs` routes the `chatgpt` provider to it, so every AI step can use the plan.
- **Before calling a change done:** `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cd src-tauri && cargo test && cargo clippy`.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

For prerequisites and troubleshooting, see [BUILD.md](BUILD.md).
The voice-detection model is checked in, so nothing needs downloading.

## Architecture Overview

Owl is a private dictation and meeting-notes desktop app, specialized in English and Chinese, built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Recording sink (VAD, levels, live streaming feed), output devices, WAV I/O
  - `vad/` - Voice Activity Detection (Silero VAD)
- `commands/` - Tauri command handlers for frontend communication
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut/` - Global keyboard shortcut handling
- `settings.rs` - Application settings management
- `overlay.rs` - Recording overlay window (platform-specific)
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `utils.rs` - Platform detection helpers

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
  - `model-selector/` - Model management interface
  - `onboarding/` - First-run experience
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta)
- `overlay/` - Recording overlay window entry point
- `lib/types.ts` - Shared TypeScript type definitions

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → VAD → Whisper/Parakeet → Text output → Clipboard/Paste

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `transcribe-cpp` - Local Whisper-family inference (GGML/GGUF) with GPU acceleration
- `transcribe-rs` - ONNX speech recognition (Parakeet, Moonshine, SenseVoice, etc.)
- `handy-recorder` - Microphone capture: device selection, resampling to 16 kHz, failure detection
- `transcribe-cpp` (Silero VAD v6, CPU-only, in-process) / `earshot` - Voice Activity Detection
- `handy-keys` - Global keyboard shortcuts
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts with its tray icon, loads settings, initializes managers
2. **First run:** `components/onboarding/`: name and what Owl is for (dictation, meetings or both), a speech model (Parakeet Unified and Nemotron Streaming are featured; it downloads from Hugging Face while setup goes on), Microphone and Accessibility (plus System Audio for meetings), the shortcut, a dictation test, an optional AI, and meeting settings. Progress is saved, so quitting midway resumes; `?mock&onboarding` previews it
3. **Recording:** Option+Space (hold to talk or tap to toggle) records with VAD filtering
4. **Processing:** The speech model transcribes, then cleanup rules, the dictionary, snippets and optional AI cleanup run
5. **Output:** Text pasted into the focused app through the clipboard; with no text box focused it stays on the clipboard instead

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable; hold or tap, hold only, or tap only)
- Audio devices (microphone/output selection)
- The selected speech model and its language
- Audio feedback and translation options

### Single Instance Architecture

Owl runs as a single instance. Launching it again brings the settings window to the front instead of starting a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX).

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
    └── ...
```

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`

## CLI Parameters

Owl supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                       | Description                                                |
| -------------------------- | ---------------------------------------------------------- |
| `--toggle-transcription`   | Toggle recording on/off on a running instance              |
| `--toggle-post-process`    | Toggle recording with post-processing on/off               |
| `--cancel`                 | Cancel the current operation on a running instance         |
| `--start-hidden`           | Launch without showing the main window (tray icon visible) |
| `--no-tray`                | Launch without system tray (closing window quits the app)  |
| `--debug`                  | Enable debug mode with verbose (Trace) logging             |
| `-f, --transcribe-file`    | Transcribe a 16 kHz mono WAV headlessly and exit           |
| `--model`, `--list-models` | Pick the model for `-f`, or list model ids                 |
| `--post-process`           | Run cleanup, snippets and the dictionary on `-f`'s text    |
| `--eval DIR`               | Replay and score a test set (`eval.rs`)                    |
| `--export-learning DIR`    | Write the learning log's fixed dictations as a test set    |
| `--learn`                  | Run the learner once and print its report                  |

Subcommands (`owl_cli.rs`, no running app needed except `transcribe`): `owl meetings [--json]`, `owl meeting <id> [--notes|--transcript]`, `owl transcribe <file>` and `owl mcp` (read-only MCP server over stdio, `mcp.rs`). Settings > Advanced > Other apps > Command line links `owl` into `/usr/local/bin` or `~/.local/bin`.

**Key design decisions:**

- CLI flags are runtime-only overrides. They don't modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: Vulkan acceleration, code signing. Implicit Vulkan layers (overlays, capture hooks) are disabled for the Owl process via `VK_LOADER_LAYERS_DISABLE=~implicit~` set in `main.rs`; opt out with `OWL_KEEP_VULKAN_IMPLICIT_LAYERS=1` or by setting `VK_LOADER_LAYERS_DISABLE` yourself
- **Linux**: OpenBLAS + Vulkan, limited Wayland support, overlay uses GTK layer shell (disable with `OWL_NO_GTK_LAYER_SHELL=1`)

## Troubleshooting

See the [Troubleshooting](README.md#troubleshooting) section in README.md.

## Commits

Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_.
