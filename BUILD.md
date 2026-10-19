# Building Owl

Owl is built and tested on Apple Silicon Macs. The Windows and Linux code it
inherits from Handy is untested with Owl's changes; Handy's own notes for those
platforms are in [Handy's repository](https://github.com/cjpais/Handy).

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/)
- Xcode Command Line Tools: `xcode-select --install`
- cmake: `brew install cmake` (the speech engine and the echo-removal library
  are built with it)

Full Xcode is only needed for Apple Intelligence cleanup. With the Command
Line Tools alone the build prints a warning and uses a stub; everything else
works.

## Build

```bash
git clone https://github.com/FOwen123/Owl.git
cd Owl
bun install
bun run tauri dev      # development
bun run tauri build    # Owl.app and a .dmg under src-tauri/target/release/bundle/
```

If cmake stops on a policy version:

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

`tauri build` first runs `scripts/build_localvqe.sh`, which clones and builds
[LocalVQE](https://github.com/localai-org/LocalVQE) for echo removal in
meetings. It needs git, cmake and a network connection, and does nothing once
the library exists. Without it, meetings work but keep the other side's echo in
your microphone track.

## Troubleshooting

### Accessibility stays on "Waiting..." after a rebuild

Local builds are signed ad hoc, so a rebuilt Owl can have a new code identity
while the old **System Settings > Privacy & Security > Accessibility** entry
still looks enabled. Quit Owl, clear only its stale record, and open it again:

```bash
osascript -e 'tell application id "com.hao.owl" to quit' || true
tccutil reset Accessibility com.hao.owl
open /Applications/Owl.app
```

Grant Accessibility again when asked. Microphone and other permissions are
not reset.

### System audio is silent in meetings

macOS grants System Audio Recording only to a bundled app, so meetings
recorded from `tauri dev` capture the microphone only. Use the built `Owl.app`
to test the other side of a call.
