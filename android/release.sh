#!/usr/bin/env bash
# Publishes the APK android/build.sh built as a GitHub release, which the
# app's updater (crates/horizon-client/src/android/update.rs) finds. The tag
# is the APK's version code, so a later build is always the newer release.
# Sign the build with the release key (HORIZON_ANDROID_KEYSTORE): Android
# installs an update only over an app signed with the same key.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
build="$here/build"
repo="${HORIZON_ANDROID_REPO:-MocLG/horizon-android}"
code=$(cat "$build/version-code")
name=$(cat "$build/version-name")
apk=$(printf '%s\n' "$build"/horizon-*-android-arm64.apk | head -n 1)
version=${name%%+*}
commit=${name#*+}
commit=${commit%%.*}

notes="Built from $commit."
if [ -n "${HORIZON_ANDROID_NOTES:-}" ]; then
    notes="$HORIZON_ANDROID_NOTES"$'\n\n'"$notes"
fi
gh release create "android-$code" "$apk" --repo "$repo" \
    --target "$(git -C "$here" rev-parse "$commit")" \
    --title "Horizon for Android $version ($commit)" \
    --notes "$notes"
