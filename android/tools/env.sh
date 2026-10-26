# Source this to cross compile the workspace for Android (aarch64) on any host.
tools=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)
export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/android-sdk}}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(printf "%s\n" "$ANDROID_HOME"/ndk/* | sort -V | tail -n 1)}"
export CC_aarch64_linux_android="$tools/ndk-clang"
export CXX_aarch64_linux_android="$tools/ndk-clang"
export AR_aarch64_linux_android="${HOST_AR:-$(command -v llvm-ar || printf "%s\n" /usr/bin/llvm-ar-* | sort -V | tail -n 1)}"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$tools/ndk-clang"
