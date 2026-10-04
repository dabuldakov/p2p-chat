#!/usr/bin/env bash
# Собирает Rust-ядро (p2pchat-ffi) под Android ABIs и раскладывает .so
# в app/src/main/jniLibs/<abi>/.
#
# Требования:
#   * Android NDK — задайте ANDROID_NDK_HOME (или ANDROID_NDK_ROOT)
#   * rustup-таргеты под Android:
#       rustup target add aarch64-linux-android x86_64-linux-android \
#                         armv7-linux-androideabi i686-linux-android
#
# Рекомендуется cargo-ndk (упрощает линковку):
#   cargo install cargo-ndk
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
JNI_DIR="$ROOT/app/src/main/jniLibs"
ABIS=("arm64-v8a" "x86_64" "armeabi-v7a" "x86")

mkdir -p "$JNI_DIR"

if command -v cargo-ndk >/dev/null 2>&1; then
    echo "==> cargo-ndk build (release)"
    cargo ndk -t arm64-v8a -t x86_64 -t armeabi-v7a -t x86 \
        -o "$JNI_DIR" build -p p2pchat-ffi --release
else
    : "${ANDROID_NDK_HOME:?Задайте ANDROID_NDK_HOME для сборки без cargo-ndk}"
    HOST_TAG="linux-x86_64"
    TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG"
    declare -A TARGETS=(
        ["arm64-v8a"]="aarch64-linux-android"
        ["x86_64"]="x86_64-linux-android"
        ["armeabi-v7a"]="armv7-linux-androideabi"
        ["x86"]="i686-linux-android"
    )
    for abi in "${ABIS[@]}"; do
        target="${TARGETS[$abi]}"
        echo "==> $abi ($target)"
        export "CC_${target//-/_}=$TOOLCHAIN/bin/${target}21-clang"
        export "AR_${target//-/_}=$TOOLCHAIN/bin/llvm-ar"
        (cd "$ROOT/.." && cargo build -p p2pchat-ffi --release --target "$target")
        mkdir -p "$JNI_DIR/$abi"
        cp "$ROOT/../target/$target/release/libp2pchat_ffi.so" "$JNI_DIR/$abi/"
    done
fi

echo "==> Готово: $JNI_DIR"
find "$JNI_DIR" -name '*.so' -print