#!/usr/bin/env bash
# Build a single-threaded Z3 wasm for verus-explorer.
#
# Self-bootstrapping: clones emsdk and Z3 into scripts/ if they're missing,
# then builds libz3.a (single-threaded, static) and links (via an empty stub
# TU, pulling Z3 API symbols directly from libz3.a) into web/z3/z3.{js,wasm}.
#
# Why single-threaded: Emscripten's -pthread build requires SharedArrayBuffer,
# which requires cross-origin isolation (COOP/COEP) on the page. Dropping
# -pthread means the page works on any static host with no special headers.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EMSDK_DIR="${HERE}/emsdk"
EMSDK_VERSION="3.1.74"
Z3_DIR="${HERE}/z3"
Z3_TAG="z3-4.16.0"
OUT_DIR="${HERE}/../web/z3"

if [[ ! -d "${EMSDK_DIR}" ]]; then
    echo "--- cloning emsdk"
    git clone --depth 1 https://github.com/emscripten-core/emsdk.git "${EMSDK_DIR}"
    "${EMSDK_DIR}/emsdk" install "${EMSDK_VERSION}"
    "${EMSDK_DIR}/emsdk" activate "${EMSDK_VERSION}"
fi

# shellcheck source=/dev/null
source "${EMSDK_DIR}/emsdk_env.sh" >/dev/null 2>&1

if [[ ! -d "${Z3_DIR}" ]]; then
    echo "--- cloning Z3 ${Z3_TAG}"
    git clone --depth 1 --branch "${Z3_TAG}" https://github.com/Z3Prover/z3.git "${Z3_DIR}"
fi

echo "--- configuring Z3 (single-threaded, static libz3.a)"
if [[ ! -f "${Z3_DIR}/build/Makefile" ]]; then
    emcmake cmake -S "${Z3_DIR}" -B "${Z3_DIR}/build" \
        -DZ3_BUILD_LIBZ3_SHARED=OFF \
        -DZ3_SINGLE_THREADED=ON \
        -DZ3_BUILD_EXECUTABLE=OFF \
        -DZ3_BUILD_TEST_EXECUTABLES=OFF \
        -DCMAKE_BUILD_TYPE=Release
fi

echo "--- building libz3.a"
cmake --build "${Z3_DIR}/build" -j"$(nproc)" --target libz3

echo "--- linking z3.{js,wasm}"
mkdir -p "${OUT_DIR}"
cd "${HERE}"

# Z3 C API symbols our JS calls via ccall. Everything else in libz3.a gets
# DCE'd out by wasm-ld. ccall handles string marshalling on the wasm stack,
# so we don't need to export _malloc/_free.
EXPORTED_FUNCTIONS='["_Z3_mk_config","_Z3_mk_context","_Z3_del_config","_Z3_eval_smtlib2_string"]'
EXPORTED_RUNTIME_METHODS='["ccall"]'

# emcc needs at least one compilation unit. We feed it an empty one via
# /dev/null — -x c forces the C language since there's no .c extension to
# sniff. All real code comes from libz3.a.
emcc -x c /dev/null "${Z3_DIR}/build/libz3.a" \
    -O2 \
    -s WASM_BIGINT \
    -s MODULARIZE=1 \
    -s "EXPORT_NAME=initZ3" \
    -s EXPORTED_FUNCTIONS="${EXPORTED_FUNCTIONS}" \
    -s EXPORTED_RUNTIME_METHODS="${EXPORTED_RUNTIME_METHODS}" \
    -s DISABLE_EXCEPTION_CATCHING=1 \
    -s INITIAL_MEMORY=64MB \
    -s ALLOW_MEMORY_GROWTH=1 \
    -s MAXIMUM_MEMORY=2GB \
    -s TOTAL_STACK=16MB \
    -o "${OUT_DIR}/z3.js"

echo "--- done"
ls -lh "${OUT_DIR}/z3.js" "${OUT_DIR}/z3.wasm"
