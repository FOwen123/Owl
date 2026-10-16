# Owl

A private dictation app for macOS that understands English and Chinese. Hold a
shortcut, talk, and Owl types what you said into whatever app you're in. It
also takes meeting notes.

> [!NOTE]
> Owl is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais and
> contributors. It is not affiliated with or endorsed by Handy. Please don't
> file Owl issues upstream. Handy's original README is kept in
> [README.handy.md](README.handy.md).

## Status

Personal and early. Built and tested on Apple Silicon Macs only. The Windows
and Linux code from Handy is still here, but nobody has tested it with Owl's
changes. There are no releases yet. To build it yourself, see
[BUILD.md](BUILD.md).

## What Owl adds to Handy

- Dictation in English and Chinese, written in Traditional or Simplified
  characters.
- Cleanup that drops filler words and tidies what you said.
- Editing selected text by voice.
- A dictionary for names and terms the speech model gets wrong. Owl learns new
  entries from the fixes you make.
- Snippets you paste by saying a phrase.
- Meeting notes with a live transcript, speaker labels and an AI summary.
- A command line and an MCP server, so other apps and AI agents can read your
  meetings.
- A new design.

## Privacy

Speech recognition runs on your Mac. AI cleanup and meeting notes are optional.
They run on your Mac with Apple Intelligence or a local server such as Ollama,
or through a provider like OpenAI or Anthropic with your own API key.

## Troubleshooting

### Shortcuts stopped working (Secure Input)

While macOS Secure Input is on, no app can see key presses, so Owl's shortcut
does nothing. Password fields, Terminal's **Secure Keyboard Entry** and some
password managers turn it on. Usually it switches off again by itself.

If it stays on, find the app holding it:

```bash
ioreg -l -w 0 | grep kCGSSessionSecureInputPID
```

Then look up that process ID with `ps -p <PID>` and quit the app, or turn off
Secure Keyboard Entry in its menu. If it is `loginwindow`, lock and unlock the
screen.

## Credits

- [Handy](https://github.com/cjpais/Handy) by CJ Pais and contributors. Owl is
  built on it.
- [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp),
  [ggml / whisper.cpp](https://github.com/ggml-org/whisper.cpp),
  [transcribe-rs](https://github.com/cjpais/transcribe-rs) and
  [Silero VAD](https://github.com/snakers4/silero-vad).
- [Streaming Sortformer v2.1](https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf)
  by NVIDIA for speaker labels.
- [LocalVQE](https://github.com/localai-org/LocalVQE) (Apache-2.0) for echo
  removal.
- Design reference: [Wispr Flow](https://wisprflow.ai).

## License

MIT, like Handy. Handy's copyright notice is kept in [LICENSE](LICENSE).
