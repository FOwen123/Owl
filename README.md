# Owl

A private, on-device dictation app for macOS that speaks English and Chinese
(Traditional and Simplified). Hold a shortcut, talk, and Owl types what you said
into whatever app you're in. It also takes meeting notes, keeps a personal
dictionary, and expands snippets.

> [!NOTE]
> Owl is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais and
> contributors. It is not affiliated with or endorsed by Handy. Please don't
> file Owl issues upstream. Handy's original README is kept in
> [README.handy.md](README.handy.md).

## Status

Personal and early. Built and tested on Apple Silicon Macs only. The Windows
and Linux code from Handy is still here but untested with Owl's changes.

## What Owl adds to Handy

- **Language switch**: English / 中文 from the tray menu, with Traditional or
  Simplified output.
- **Snippets**: say a trigger phrase, Owl pastes the saved text.
- **Dictionary**: your words, plus "heard → write" replacements that also work
  for Chinese. When you fix a misheard word after dictating, Owl suggests a
  replacement.
- **Notetaker**: record a meeting (microphone and system audio), get a
  transcript and an optional AI summary.
- **A redesign**: see [DESIGN.md](DESIGN.md).

The plan and progress live in [TODO.md](TODO.md).

## Privacy

Speech recognition runs on your Mac. Text cleanup and meeting summaries are
optional; they run on your Mac (Apple Intelligence, Ollama) or with an API key
you provide.

## Building

You need [Rust](https://rustup.rs), [bun](https://bun.sh), cmake
(`brew install cmake`) and Xcode.

```bash
bun install
mkdir -p src-tauri/resources/models
curl -o src-tauri/resources/models/silero_vad_v4.onnx https://blob.handy.computer/silero_vad_v4.onnx
bun run tauri dev
```

In dev mode, macOS asks the app that launched Owl (your terminal) for
Microphone and Accessibility permission. If Accessibility looks enabled but
pasting doesn't work after a rebuild, run
`tccutil reset Accessibility com.hao.owl`.

More detail: [BUILD.md](BUILD.md). Notes for AI coding assistants:
[AGENTS.md](AGENTS.md).

## Troubleshooting

### Shortcuts stopped working (Secure Input)

While macOS Secure Input is on, no app can see key presses, so Owl's shortcut
does nothing. Password fields, Terminal's **Secure Keyboard Entry** and some
password managers turn it on; usually it switches off again by itself.

If it stays on, find the app holding it:

```bash
ioreg -l -w 0 | grep kCGSSessionSecureInputPID
```

Then look up that process ID with `ps -p <PID>` and quit the app, or turn off
Secure Keyboard Entry in its menu. If it is `loginwindow`, lock and unlock the
screen.

## Keeping up with Handy

```bash
git fetch upstream
git merge upstream/main
```

Owl only renames what users see; internal names (`handy` crate, `HANDY_*`
environment variables) stay the same to keep these merges small.

## Credits

- [Handy](https://github.com/cjpais/Handy) by CJ Pais and contributors, the
  foundation of everything here.
- [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp),
  [ggml / whisper.cpp](https://github.com/ggml-org/whisper.cpp),
  [transcribe-rs](https://github.com/cjpais/transcribe-rs) and
  [Silero VAD](https://github.com/snakers4/silero-vad).
- Design references: [Superwhisper](https://superwhisper.com),
  [T3 Code](https://github.com/pingdotgg/t3code) and
  [Wispr Flow](https://wisprflow.ai).

## License

MIT, like Handy. Handy's copyright notice is kept in [LICENSE](LICENSE).
