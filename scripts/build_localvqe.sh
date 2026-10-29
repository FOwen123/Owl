#!/bin/sh
# Owl: builds LocalVQE (Apache-2.0, https://github.com/localai-org/LocalVQE)
# into src-tauri/resources/localvqe/liblocalvqe.dylib (localvqe.dll on
# Windows, from Git Bash), which removes the call's sound from the microphone
# in meetings. Runs before `tauri build`;
# does nothing when the library is already there. Without it, meetings work
# as before, just without echo removal.
set -e
COMMIT=f53063c9eb2a85f96479867d1dd911dc3bf6319b
OUT="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources/localvqe"
case "$(uname)" in
  Darwin)
    LIB="$OUT/liblocalvqe.dylib"
    # CPU only: Metal adds ~5 s of start-up and nothing for a model this small.
    # macOS 13.3 is the oldest with the BLAS calls ggml uses; on older systems
    # the library won't load and meetings skip echo removal.
    FLAGS="-DGGML_METAL=OFF -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3"
    ;;
  MINGW* | MSYS*)
    LIB="$OUT/localvqe.dll"
    # One self-contained DLL: no OpenMP or C runtime DLLs to ship, and SSE4.2
    # code that runs on any 64-bit PC (the model is too small to need AVX).
    FLAGS="-DGGML_OPENMP=OFF -DGGML_AVX=OFF -DGGML_AVX2=OFF -DGGML_BMI2=OFF -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"
    ;;
  *)
    echo "LocalVQE: skipped (only built on macOS and Windows for now)"
    exit 0
    ;;
esac

# Tauri signs the app but not its resources, and a signed Mac build won't load
# or notarize a library without the same Developer ID, so sign it here.
sign() {
  [ "$(uname)" = Darwin ] && [ -n "$APPLE_SIGNING_IDENTITY" ] && [ "$APPLE_SIGNING_IDENTITY" != "-" ] || return 0
  codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$LIB"
  echo "LocalVQE: signed $LIB"
}

[ -f "$LIB" ] && { sign; exit 0; }

SRC="$(mktemp -d)"
trap 'rm -rf "$SRC"' EXIT
git clone --quiet https://github.com/localai-org/LocalVQE "$SRC"
git -C "$SRC" checkout --quiet "$COMMIT"
git -C "$SRC" submodule update --quiet --init --depth 1
LOG="$SRC/build.log"
# On x86 PCs LocalVQE splits ggml into per-CPU DLLs it looks for next to the
# program, where Owl's own ggml DLLs already sit. Take the build it uses on
# other CPUs, with ggml inside the library, as on macOS.
case "$LIB" in *.dll) sed -i 's/x86_64|AMD64|amd64/x86_64-unused/' "$SRC/ggml/CMakeLists.txt" ;; esac
# $FLAGS is split into separate options on purpose.
# shellcheck disable=SC2086
if ! { cmake -S "$SRC/ggml" -B "$SRC/build" -DCMAKE_BUILD_TYPE=Release \
  -DLOCALVQE_BUILD_SHARED=ON $FLAGS &&
  cmake --build "$SRC/build" --config Release --target localvqe_shared -j 8; } > "$LOG" 2>&1; then
  tail -20 "$LOG"
  exit 1
fi
mkdir -p "$OUT"
case "$LIB" in
  *.dll) cp "$(find "$SRC/build" -name localvqe.dll | head -1)" "$LIB" ;;
  *)
    cp "$SRC/build/liblocalvqe.0.1.0.dylib" "$LIB"
    install_name_tool -id @rpath/liblocalvqe.dylib "$LIB"
    ;;
esac
echo "LocalVQE: built $LIB"
sign
