#!/usr/bin/env bash
set -euo pipefail

# Fabushi Android — Native Rust Host Builder (`libmahayana_app_host.so`)
# Builds:
#   1. aarch64-linux-android -> mobile/android/app/src/main/jniLibs/arm64-v8a/libmahayana_app_host.so (16KB page-aligned)
#   2. x86_64-linux-android  -> mobile/android/app/src/main/jniLibs/x86_64/libmahayana_app_host.so    (16KB page-aligned)
#   3. x86_64-unknown-linux-gnu -> mobile/android/rust/host-lib/libmahayana_app_host.so               (Host JVM JNI tests)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="${REPO_ROOT}/mobile/android/rust/mahayana_android_host"
JNILIBS_DIR="${REPO_ROOT}/mobile/android/app/src/main/jniLibs"
HOST_LIB_DIR="${REPO_ROOT}/mobile/android/rust/host-lib"
ANDROID_API_LEVEL="${ANDROID_API_LEVEL:-26}"

if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
  echo "ERROR: cargo not found in PATH or ~/.cargo/bin/cargo" >&2
  exit 1
fi

resolve_ndk_home() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -d "${ANDROID_NDK_HOME}" ]]; then
    echo "${ANDROID_NDK_HOME}"
    return 0
  fi
  if [[ -n "${ANDROID_NDK_ROOT:-}" && -d "${ANDROID_NDK_ROOT}" ]]; then
    echo "${ANDROID_NDK_ROOT}"
    return 0
  fi
  for sdk_candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "${HOME}/android-sdk" "${HOME}/Android/Sdk" "/usr/local/lib/android/sdk"; do
    if [[ -n "${sdk_candidate}" && -d "${sdk_candidate}/ndk" ]]; then
      local latest
      latest="$(ls -1 "${sdk_candidate}/ndk" 2>/dev/null | sort -V | tail -n 1 || true)"
      if [[ -n "${latest}" && -d "${sdk_candidate}/ndk/${latest}" ]]; then
        echo "${sdk_candidate}/ndk/${latest}"
        return 0
      fi
    fi
  done
  return 1
}

NDK_HOME="$(resolve_ndk_home || true)"
if [[ -z "${NDK_HOME}" ]]; then
  echo "ERROR: Android NDK not found. Set ANDROID_NDK_HOME or install NDK under ANDROID_HOME/ndk." >&2
  exit 1
fi

TOOLCHAIN_BIN=""
for host_tag in linux-x86_64 darwin-arm64 darwin-x86_64; do
  if [[ -d "${NDK_HOME}/toolchains/llvm/prebuilt/${host_tag}/bin" ]]; then
    TOOLCHAIN_BIN="${NDK_HOME}/toolchains/llvm/prebuilt/${host_tag}/bin"
    break
  fi
done

if [[ -z "${TOOLCHAIN_BIN}" ]]; then
  echo "ERROR: LLVM prebuilt toolchain bin not found under ${NDK_HOME}/toolchains/llvm/prebuilt" >&2
  exit 1
fi

echo "==> Using Cargo: ${CARGO_BIN}"
echo "==> Using Android NDK: ${NDK_HOME} (API ${ANDROID_API_LEVEL})"

# Configure C/C++ compilers, archivers, linkers, and 16KB ELF page-size flags for Android targets
export CC_aarch64_linux_android="${TOOLCHAIN_BIN}/aarch64-linux-android${ANDROID_API_LEVEL}-clang"
export CXX_aarch64_linux_android="${TOOLCHAIN_BIN}/aarch64-linux-android${ANDROID_API_LEVEL}-clang++"
export AR_aarch64_linux_android="${TOOLCHAIN_BIN}/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${CC_aarch64_linux_android}"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"

export CC_x86_64_linux_android="${TOOLCHAIN_BIN}/x86_64-linux-android${ANDROID_API_LEVEL}-clang"
export CXX_x86_64_linux_android="${TOOLCHAIN_BIN}/x86_64-linux-android${ANDROID_API_LEVEL}-clang++"
export AR_x86_64_linux_android="${TOOLCHAIN_BIN}/llvm-ar"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${CC_x86_64_linux_android}"
export CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"

mkdir -p "${JNILIBS_DIR}/arm64-v8a" "${JNILIBS_DIR}/x86_64" "${HOST_LIB_DIR}"

echo "==> [1/3] Building release cdylib for aarch64-linux-android (arm64-v8a, 16KB page-aligned)..."
"${CARGO_BIN}" build \
  --manifest-path "${CRATE_DIR}/Cargo.toml" \
  --release \
  --target aarch64-linux-android
cp -f "${CRATE_DIR}/target/aarch64-linux-android/release/libmahayana_app_host.so" \
  "${JNILIBS_DIR}/arm64-v8a/libmahayana_app_host.so"

echo "==> [2/3] Building release cdylib for x86_64-linux-android (x86_64, 16KB page-aligned)..."
"${CARGO_BIN}" build \
  --manifest-path "${CRATE_DIR}/Cargo.toml" \
  --release \
  --target x86_64-linux-android
cp -f "${CRATE_DIR}/target/x86_64-linux-android/release/libmahayana_app_host.so" \
  "${JNILIBS_DIR}/x86_64/libmahayana_app_host.so"

echo "==> [3/3] Building release cdylib for host JVM (x86_64-unknown-linux-gnu)..."
"${CARGO_BIN}" build \
  --manifest-path "${CRATE_DIR}/Cargo.toml" \
  --release \
  --target x86_64-unknown-linux-gnu
cp -f "${CRATE_DIR}/target/x86_64-unknown-linux-gnu/release/libmahayana_app_host.so" \
  "${HOST_LIB_DIR}/libmahayana_app_host.so"

echo "==> Cleaning intermediate Cargo build/deps/incremental artifacts to keep target/ compact..."
rm -rf \
  "${CRATE_DIR}/target/debug" \
  "${CRATE_DIR}/target/release/deps" \
  "${CRATE_DIR}/target/release/build" \
  "${CRATE_DIR}/target/release/incremental" \
  "${CRATE_DIR}/target/aarch64-linux-android/release/deps" \
  "${CRATE_DIR}/target/aarch64-linux-android/release/build" \
  "${CRATE_DIR}/target/aarch64-linux-android/release/incremental" \
  "${CRATE_DIR}/target/x86_64-linux-android/release/deps" \
  "${CRATE_DIR}/target/x86_64-linux-android/release/build" \
  "${CRATE_DIR}/target/x86_64-linux-android/release/incremental" \
  "${CRATE_DIR}/target/x86_64-unknown-linux-gnu/release/deps" \
  "${CRATE_DIR}/target/x86_64-unknown-linux-gnu/release/build" \
  "${CRATE_DIR}/target/x86_64-unknown-linux-gnu/release/incremental"

echo "==> Built native libraries:"
ls -lh \
  "${JNILIBS_DIR}/arm64-v8a/libmahayana_app_host.so" \
  "${JNILIBS_DIR}/x86_64/libmahayana_app_host.so" \
  "${HOST_LIB_DIR}/libmahayana_app_host.so"
