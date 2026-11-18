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
ANDROID_API_LEVEL="${ANDROID_API_LEVEL:-29}"

if [[ -d "${HOME}/.rustup/toolchains" ]]; then
  if [[ -z "${RUSTUP_HOME:-}" || ! -w "${RUSTUP_HOME}" ]]; then
    export RUSTUP_HOME="${HOME}/.rustup"
  fi
fi
if [[ -d "${HOME}/.cargo/bin" ]]; then
  export CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi

if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
  export PATH="${HOME}/.cargo/bin:${PATH}"
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
  echo "ERROR: cargo not found in PATH or ~/.cargo/bin/cargo" >&2
  exit 1
fi

read_local_properties_sdk_dir() {
  local props_file="${REPO_ROOT}/mobile/android/local.properties"
  if [[ -f "${props_file}" ]]; then
    sed -n 's/^[[:space:]]*sdk\.dir[[:space:]]*=[[:space:]]*//p' "${props_file}" \
      | tail -n 1 \
      | sed 's/\\:/:/g; s/\\\\/\\/g; s/[[:space:]]*$//'
  fi
}

resolve_ndk_home() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -d "${ANDROID_NDK_HOME}" ]]; then
    echo "${ANDROID_NDK_HOME}"
    return 0
  fi
  if [[ -n "${ANDROID_NDK_ROOT:-}" && -d "${ANDROID_NDK_ROOT}" ]]; then
    echo "${ANDROID_NDK_ROOT}"
    return 0
  fi
  local local_sdk_dir
  local_sdk_dir="$(read_local_properties_sdk_dir || true)"
  for sdk_candidate in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "${local_sdk_dir}" "${HOME}/android-sdk" "${HOME}/Android/Sdk" "/usr/local/lib/android/sdk"; do
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

verify_elf_16kb_alignment() {
  python3 - "$@" <<'PY'
import pathlib
import struct
import sys

ELF_MAGIC = b"\x7fELF"
ELFCLASS64 = 2
ELFDATA2LSB = 1
ELF_ET_DYN = 3
ELF_EM_X86_64 = 62
ELF_EM_AARCH64 = 183
ELF64_EHDR_SIZE = 64
ELF64_PHDR_MIN_SIZE = 56
ELF_PT_LOAD = 1
MIN_PAGE_ALIGN_16KB = 16384
ABI_EXPECTED_MACHINE = {
    "arm64-v8a": ELF_EM_AARCH64,
    "x86_64": ELF_EM_X86_64,
}


def check_elf_16kb_alignment(
    elf_bytes: bytes,
    label: str,
    *,
    expected_machine: int | None = None,
) -> list[tuple[int, int]]:
    if not isinstance(elf_bytes, (bytes, bytearray, memoryview)):
        raise RuntimeError(f"{label} ELF payload must be bytes-like")
    buf = memoryview(elf_bytes).cast("B")
    total_len = len(buf)
    if total_len < ELF64_EHDR_SIZE or buf[:4].tobytes() != ELF_MAGIC:
        raise RuntimeError(f"{label} is not a valid 64-bit ELF shared library (size={total_len} bytes)")
    if buf[4] != ELFCLASS64 or buf[5] != ELFDATA2LSB:
        raise RuntimeError(
            f"{label} is not a 64-bit little-endian ELF shared library "
            f"(ei_class={buf[4]}, ei_data={buf[5]})"
        )

    e_type, e_machine = struct.unpack_from("<HH", buf, 16)
    if e_type != ELF_ET_DYN:
        raise RuntimeError(f"{label} must be an ET_DYN shared object (e_type=3), got e_type={e_type}")
    if expected_machine is not None:
        if e_machine != expected_machine:
            raise RuntimeError(
                f"{label} has e_machine={e_machine}, expected {expected_machine}"
            )
    elif e_machine not in (ELF_EM_AARCH64, ELF_EM_X86_64):
        raise RuntimeError(
            f"{label} has unsupported 64-bit Android e_machine={e_machine} "
            f"(expected {ELF_EM_AARCH64} EM_AARCH64 or {ELF_EM_X86_64} EM_X86_64)"
        )

    e_phoff = struct.unpack_from("<Q", buf, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", buf, 54)
    if e_phentsize < ELF64_PHDR_MIN_SIZE:
        raise RuntimeError(
            f"{label} has invalid e_phentsize={e_phentsize} (must be >= {ELF64_PHDR_MIN_SIZE})"
        )
    if e_phnum <= 0:
        raise RuntimeError(f"{label} has invalid e_phnum={e_phnum} (must be > 0)")
    if e_phoff < ELF64_EHDR_SIZE:
        raise RuntimeError(
            f"{label} has invalid e_phoff={e_phoff} (must be >= {ELF64_EHDR_SIZE})"
        )
    phdr_table_end = e_phoff + e_phnum * e_phentsize
    if phdr_table_end > total_len:
        raise RuntimeError(
            f"{label} program header table [0x{e_phoff:x}..0x{phdr_table_end:x}] "
            f"exceeds ELF size ({total_len} bytes)"
        )

    pt_load_aligns: list[tuple[int, int]] = []
    for idx in range(e_phnum):
        off = e_phoff + idx * e_phentsize
        p_type = struct.unpack_from("<I", buf, off)[0]
        if p_type == ELF_PT_LOAD:
            p_align = struct.unpack_from("<Q", buf, off + 48)[0]
            if p_align < MIN_PAGE_ALIGN_16KB or (p_align & (p_align - 1)) != 0:
                raise RuntimeError(
                    f"{label} PT_LOAD segment #{idx} has invalid p_align=0x{p_align:x} "
                    f"(must be a power of two >= 0x4000 / 16KB)"
                )
            pt_load_aligns.append((idx, p_align))
    if not pt_load_aligns:
        raise RuntimeError(f"{label} contains zero PT_LOAD segments")
    return pt_load_aligns


def main(argv: list[str]) -> int:
    if not argv:
        raise RuntimeError("No ELF binary paths provided to verify_elf_16kb_alignment")
    for raw_path in argv:
        path = pathlib.Path(raw_path)
        if not path.is_file():
            raise RuntimeError(f"Expected ELF binary does not exist: {path}")
        expected_machine = ABI_EXPECTED_MACHINE.get(path.parent.name)
        pt_load_aligns = check_elf_16kb_alignment(
            path.read_bytes(),
            str(path),
            expected_machine=expected_machine,
        )
        summary = ", ".join(f"#{idx}=0x{align:x}" for idx, align in pt_load_aligns)
        print(f"    [16KB ELF OK] {path.parent.name}/{path.name}: PT_LOAD p_align [{summary}]")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
PY
}

NDK_HOME="$(resolve_ndk_home || true)"
if [[ -z "${NDK_HOME}" ]]; then
  echo "ERROR: Android NDK not found. Set ANDROID_NDK_HOME, configure sdk.dir in mobile/android/local.properties, or install NDK under ANDROID_HOME/ndk." >&2
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

# Configure C/C++ compilers, archivers, linkers, bounded jobs, and 16KB ELF page-size flags for Android targets
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
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

echo "==> Verifying 16KB ELF PT_LOAD alignment (p_align >= 0x4000) on Android native libraries..."
verify_elf_16kb_alignment \
  "${JNILIBS_DIR}/arm64-v8a/libmahayana_app_host.so" \
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
