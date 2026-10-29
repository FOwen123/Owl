# Building Owl

The [README](README.md#install) has the short version. This page has the
details: what each tool is for, what the build does, how to work on Owl, and
what to do when a build fails.

Owl is built and tested on macOS. The Windows and Linux code from
Handy is untested; Handy's own notes for those are in
[its repository](https://github.com/cjpais/Handy).

## Requirements

### All platforms

- [Rust](https://rustup.rs/) 1.99 or newer
- [Bun](https://bun.sh/) 1.4 or newer
- [CMake](https://cmake.org/download/), which builds the speech engine and the
  echo-removal library
- [Tauri's prerequisites](https://tauri.app/start/prerequisites/)

### macOS

- Xcode Command Line Tools: `xcode-select --install`
- CMake through [Homebrew](https://brew.sh): `brew install cmake`

Some features need a newer macOS:

| Feature                           | macOS |
| --------------------------------- | ----- |
| Echo removal in meetings          | 13.3  |
| Hearing the other side of a call  | 14.2  |
| Offering notes when a call starts | 14.4  |
| Apple Intelligence cleanup        | 26    |

## What the build does

`bun install` downloads the interface's packages, about 230 MB.

`bun run tauri build` then:

1. Builds `liblocalvqe.dylib` for echo removal, from
   [LocalVQE](https://github.com/localai-org/LocalVQE) at a pinned commit. This
   needs git, CMake and a network connection once. After that it's skipped,
   except for signing it when you make a [signed build](#signed-builds).
2. Builds the interface with `tsc` and Vite.
3. Compiles the Rust app, about 870 crates the first time.
4. Writes `Owl.app` to `src-tauri/target/release/bundle/macos/` and a `.dmg`
   to `bundle/dmg/`. Both are signed ad hoc unless you make a
   [signed build](#signed-builds).

The build doesn't download speech models. Owl does that when you pick one.

## Development

```bash
bun install
bun run tauri dev
```

This runs a debug build. Interface changes reload on their own, and Rust
changes restart the app. Meetings recorded this way get no call audio, since
macOS only gives System Audio Recording to a bundled app.

To work on the interface without Rust, run `bun run dev` and open
`http://localhost:1420/?mock`. The recording overlay is at
`http://localhost:1420/src/overlay/index.html?mock`, and `&state=transcribing`
or `&state=notice` shows its other states. The fake data is in
`src/dev/mockBackend.ts`.

Run these before you call a change done:

```bash
bun run lint
bunx tsc --noEmit
bun run check:translations
cd src-tauri && cargo test && cargo clippy
```

To test without touching your own settings and history, copy `Owl.app`
somewhere and mark the copy as portable. It then keeps its data in a `Data`
folder next to the binary.

```bash
printf 'Owl Portable Mode' > /path/to/copy/Owl.app/Contents/MacOS/portable
```

## Build settings

| Variable                           | What it does                                                         |
| ---------------------------------- | -------------------------------------------------------------------- |
| `CMAKE_POLICY_VERSION_MINIMUM=3.5` | Lets new CMake build dependencies that ask for an old policy version |
| `OWL_FORCE_AI_STUB=1`              | Leaves Apple Intelligence out even when Xcode is installed           |
| `SDKROOT`, `SWIFTC`                | Pick the SDK or Swift compiler for the Apple Intelligence bridge     |

Apple Intelligence is the one part that needs full Xcode. Without Xcode the
build prints a warning and leaves it out. Every other AI provider still works.

## Signed builds

A `.dmg` that opens on other Macs needs a "Developer ID Application"
certificate in your keychain and an
[app-specific password](https://account.apple.com) for notarization.

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com" APPLE_TEAM_ID=TEAMID
read -s APPLE_PASSWORD; export APPLE_PASSWORD
bun run tauri build
```

This signs `liblocalvqe.dylib` and the app, sends the app to Apple and staples
the result. Then notarize and staple the `.dmg` itself:

```bash
DMG=$(ls src-tauri/target/release/bundle/dmg/*.dmg)
xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD" --wait
xcrun stapler staple "$DMG"
spctl -a -vvv -t open --context context:primary-signature "$DMG"
```

## When the build fails

**`xcrun: error: invalid active developer path`.** The Command Line Tools are
missing, which often happens after a macOS update. Run `xcode-select --install`
and build again.

**"Compatibility with CMake < 3.5 has been removed".** Build with
`CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build`.

**The LocalVQE step fails.** It needs git, CMake and access to github.com the
first time. Fix whichever is missing and build again.

**Accessibility stays on "Waiting..." after a rebuild.** Every local build has
a new code signature, so the old Accessibility entry no longer applies even
though it still looks on. Clear it and allow Owl again:

```bash
osascript -e 'tell application id "com.hao.owl" to quit' || true
tccutil reset Accessibility com.hao.owl
open /Applications/Owl.app
```

This only resets Accessibility. Microphone and other permissions stay.
