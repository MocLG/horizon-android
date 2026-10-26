#!/usr/bin/env bash
# Builds the Android app: the game's library, the java launcher, the Java
# runtime and Horizon's version folder as assets, then the APK.
#
#   android/build.sh            release APK in android/build/
#   HORIZON_ANDROID_KEYSTORE=... signs it with your key (else the debug key)
#
# Needs Rust with the aarch64-linux-android target, clang and lld, the
# Android SDK and NDK (ANDROID_HOME), JDK 17 or newer for Gradle, xz, zip
# and python3. On a host whose SDK tools are x86_64 only, qemu runs aapt2.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
build="$here/build"
stage="$build/stage"
cache="$build/cache"
. "$here/tools/env.sh"

# The Java runtime: Zalith Launcher 2's OpenJDK 25 for Android (GPLv2 with
# the Classpath Exception), pinned by commit and checked.
jre_commit=fa4103be9b69ab81d49b459b9477d38af397f038
jre_url="https://raw.githubusercontent.com/ZalithLauncher/ZalithLauncher2/$jre_commit/ZalithLauncher/src/main/assets/runtimes/jre-25"
jre_files=(
    "universal.tar.xz 23141bf4e0ed46ff0b751c6cc32b3863d39e6a37b720f3d86a4fe9318aeb8e9f"
    "bin-arm64.tar.xz 16f09d1d163c008ae556be68c624057c4791c21ae6d31302334e61d75439567b"
)

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/horizon-client/Cargo.toml" | head -n 1)
commit=$(git -C "$repo" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
export HORIZON_ANDROID_BUILD="${HORIZON_ANDROID_BUILD:-$version+$commit.$(date -u +%Y%m%d%H%M)}"
jobs="${HORIZON_ANDROID_JOBS:-4}"
# The Azure app players sign in through (public; Settings can override it).
export HORIZON_MSA_CLIENT_ID="${HORIZON_MSA_CLIENT_ID:-5be5d759-7f2f-4790-be62-438977cdc7a5}"

rm -rf "$stage"
mkdir -p "$stage/jniLibs/arm64-v8a" "$stage/assets" "$cache"
libs="$stage/jniLibs/arm64-v8a"
strip=$(command -v llvm-strip || printf '%s\n' /usr/bin/llvm-strip-* | sort -V | tail -n 1)

echo "== The game ($HORIZON_ANDROID_BUILD)"
cargo build --manifest-path "$repo/Cargo.toml" -p horizon-client --example horizon \
    --target aarch64-linux-android --profile android -j "$jobs" \
    --no-default-features --features audio,android
"$strip" --strip-debug -o "$libs/libhorizon.so" \
    "$repo/target/aarch64-linux-android/android/examples/libhorizon.so"

echo "== The java launcher"
cargo build --manifest-path "$here/java-launcher/Cargo.toml" --release --target aarch64-linux-android
launcher="$here/java-launcher/target/aarch64-linux-android/release/horizon-java-launcher"
cp "$launcher" "$libs/libhorizonjava.so"
cp "$launcher" "$libs/libhorizonjavac.so"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$libs/"

echo "== The Java runtime"
jre_zip="$cache/jre-25-$jre_commit.zip"
if [ ! -f "$jre_zip" ]; then
    jre_dir="$cache/jre-25"
    rm -rf "$jre_dir" && mkdir -p "$jre_dir"
    for entry in "${jre_files[@]}"; do
        read -r name sum <<<"$entry"
        file="$cache/$jre_commit-$name"
        [ -f "$file" ] || curl -fsSL -o "$file" "$jre_url/$name"
        echo "$sum  $file" | sha256sum -c --quiet
        tar -xJf "$file" -C "$jre_dir"
    done
    (cd "$jre_dir" && zip -qr -X "$jre_zip.partial" .)
    mv "$jre_zip.partial" "$jre_zip"
fi
cp "$jre_zip" "$stage/assets/jre-25.zip"

echo "== Horizon's version folder"
root="$build/root"
rm -rf "$root" && mkdir -p "$root"
(cd "$repo" && cp --parents -r research-lock.json LICENSE crates/horizon-prepare/Cargo.toml \
    crates/horizon-prepare/build.rs crates/horizon-prepare/src \
    mods/horizon/src/test/java/dev/horizonlab/parity "$root/")
# Exporter classes of a release built from the same sources save the phone
# compiling them. Without a match the phone compiles them on its first run.
if [ -n "${HORIZON_EXPORTERS:-}" ]; then
    cp -r "$HORIZON_EXPORTERS" "$root/exporters"
fi
if [ -d "$root/exporters" ]; then
    want=$(python3 -I "$here/tools/exporter_key.py" "$root")
    have=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["key"])' "$root/exporters/manifest.json")
    if [ "$want" != "$have" ]; then
        echo "The exporters in $HORIZON_EXPORTERS were built from other sources: leaving them out"
        rm -rf "$root/exporters"
    fi
fi
(cd "$root" && zip -qr -X "$stage/assets/horizon.zip" .)

echo "== The APK"
cd "$here"
aapt2=()
if [ "$(uname -m)" != x86_64 ]; then
    aapt2=("-Pandroid.aapt2FromMavenOverride=$here/tools/aapt2")
fi
echo "sdk.dir=$ANDROID_HOME" > local.properties
./gradlew --no-daemon -q :app:assembleRelease "${aapt2[@]}" \
    -PhorizonVersionName="$version" \
    -PhorizonVersionCode="$(date -u +%s | cut -c1-9)"
apk="$build/horizon-$version-android-arm64.apk"
cp app/build/outputs/apk/release/app-release.apk "$apk"
echo "Built $apk"
