#!/usr/bin/env bash
# One-time setup for verus-explorer.
#
# Installs the wasm32 rust target, clones emsdk and Z3 into third_party/,
# and installs+activates the pinned emsdk version. Idempotent — safe to
# re-run. Run this once after a fresh checkout; the Makefile drives
# everything else.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="${HERE}/third_party"
EMSDK_DIR="${VENDOR_DIR}/emsdk"
EMSDK_VERSION="3.1.74"
Z3_DIR="${VENDOR_DIR}/z3"
Z3_TAG="z3-4.16.0"

echo "--- adding wasm32-unknown-unknown rust target"
rustup target add wasm32-unknown-unknown

mkdir -p "${VENDOR_DIR}"

if [[ ! -d "${EMSDK_DIR}" ]]; then
    echo "--- cloning emsdk"
    git clone --depth 1 https://github.com/emscripten-core/emsdk.git "${EMSDK_DIR}"
fi
echo "--- installing+activating emsdk ${EMSDK_VERSION}"
"${EMSDK_DIR}/emsdk" install "${EMSDK_VERSION}"
"${EMSDK_DIR}/emsdk" activate "${EMSDK_VERSION}"

if [[ ! -d "${Z3_DIR}" ]]; then
    echo "--- cloning Z3 ${Z3_TAG}"
    git clone --depth 1 --branch "${Z3_TAG}" https://github.com/Z3Prover/z3.git "${Z3_DIR}"
fi

echo "--- setup done"
