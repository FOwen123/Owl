#!/bin/sh
# Owl: builds LocalVQE (Apache-2.0, https://github.com/localai-org/LocalVQE)
# into src-tauri/resources/localvqe/liblocalvqe.dylib, which removes the
# call's sound from the microphone in meetings. Runs before `tauri build`;
# does nothing when the library is already there. Without it, meetings work
# as before, just without echo removal.
set -e
COMMIT=f53063c9eb2a85f96479867d1dd911dc3bf6319b
OUT="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/localvqe"
LIB="$OUT/liblocalvqe.dylib"

# Tauri signs the app but not its resources, and a signed build won't load
# or notarize a library without the same Developer ID, so sign it here.
sign() {
  [ -n "$APPLE_SIGNING_IDENTITY" ] && [ "$APPLE_SIGNING_IDENTITY" != "-" ] || return 0
  codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$LIB"
  echo "LocalVQE: signed $LIB"
}

[ -f "$LIB" ] && { sign; exit 0; }
if [ "$(uname)" != "Darwin" ]; then
  echo "LocalVQE: skipped (only built on macOS for now)"
  exit 0
fi

SRC="$(mktemp -d)"
trap 'rm -rf "$SRC"' EXIT
git clone --quiet https://github.com/localai-org/LocalVQE "$SRC"
git -C "$SRC" checkout --quiet "$COMMIT"
git -C "$SRC" submodule update --quiet --init --depth 1
LOG="$SRC/build.log"
# CPU only: Metal adds ~5 s of start-up and nothing for a model this small.
# macOS 13.3 is the oldest with the BLAS calls ggml uses; on older systems the
# library won't load and meetings skip echo removal.
if ! { cmake -S "$SRC/ggml" -B "$SRC/build" -DCMAKE_BUILD_TYPE=Release \
  -DLOCALVQE_BUILD_SHARED=ON -DGGML_METAL=OFF -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3 &&
  cmake --build "$SRC/build" --target localvqe_shared -j 8; } > "$LOG" 2>&1; then
  tail -20 "$LOG"
  exit 1
fi
mkdir -p "$OUT"
cp "$SRC/build/liblocalvqe.0.1.0.dylib" "$LIB"
install_name_tool -id @rpath/liblocalvqe.dylib "$LIB"
echo "LocalVQE: built $LIB"
sign
