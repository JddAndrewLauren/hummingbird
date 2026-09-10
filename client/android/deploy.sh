#!/bin/bash
# Build the release APK and drop it in Dropbox, where the phone installs it
# from -- the whole deploy path, and the one that needs no Developer options
# on the device.
#
#   ./client/android/deploy.sh
#
# Why this shape (2026-09-10): `installDebug` needs USB debugging, and so
# Developer options, which a banking app on the phone objects to. Android's
# package installer needs neither: a file the phone downloads itself installs
# with a tap, and installs *over* the previous build in place -- keeping the
# device token and the outbound queue -- provided two things hold. The
# signing key is the same one every time, and `versionCode` never goes
# backwards. Debug builds broke the first (a debug key is per machine and per
# CI run; `README.md`'s date-picker pass records the uninstall that cost),
# so this script signs with the one release key, held in 1Password and never
# rotated; `app/build.gradle.kts` derives the second from the commit count.
#
# The key follows the ADMIN_SECRET handling rule (CLAUDE.md, "Credential
# blast radius"): it is fetched from `op://dev/hummingbird-android-keystore`
# into a temp dir for the length of one build and deleted on exit, along with
# the `keystore.properties` written beside the Gradle root. Neither ever goes
# in Actions -- `android.yml`'s header says why CI stays debug-only.
#
# An `op read` that hangs means the approval prompt went unanswered, not that
# auth failed. Nothing here retries; answer the prompt or come back later.
#
# The keystore is PKCS12 (keytool's default), whose key password *is* the
# store password: keytool silently ignores a different `-keypass`, and the
# first build off a two-password item died in `packageRelease` with "Given
# final block not properly padded". The item carries `keyPassword` anyway,
# equal to `password`, because that is the contract `keystore.properties`
# and the Gradle file speak; if the key is ever re-made, keep them equal.
#
# Env: HB_APK_DIR overrides the Dropbox target (default
# ~/Dropbox/hummingbird/apk); HB_KEYSTORE_ITEM overrides the 1Password item.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
item=${HB_KEYSTORE_ITEM:-hummingbird-android-keystore}
apk_dir=${HB_APK_DIR:-$HOME/Dropbox/hummingbird/apk}

for cmd in op; do
  command -v "$cmd" > /dev/null || { echo "$cmd not found on PATH" >&2; exit 2; }
done

tmp=$(mktemp -d)
props="$here/keystore.properties"
# This script owns that file for the length of one build and deletes it on
# exit; a pre-existing one is somebody's hand-made signing setup, not ours
# to remove.
[ -e "$props" ] && { echo "$props already exists; move it aside first" >&2; exit 2; }
cleanup() { rm -rf "$tmp" "$props"; }
trap cleanup EXIT

umask 077
# The passwords are read straight into variables and written only to the
# properties file below: never argv, never the shell history.
op read "op://dev/$item/keystore" --out-file "$tmp/release.jks" > /dev/null
store_pw=$(op read "op://dev/$item/password")
key_pw=$(op read "op://dev/$item/keyPassword")
key_alias=$(op read "op://dev/$item/keyAlias")
[ -s "$tmp/release.jks" ] && [ -n "$store_pw" ] && [ -n "$key_pw" ] && [ -n "$key_alias" ] \
  || { echo "1Password item '$item' is missing the keystore or a field" >&2; exit 2; }

cat > "$props" <<PROPS
storeFile=$tmp/release.jks
storePassword=$store_pw
keyAlias=$key_alias
keyPassword=$key_pw
PROPS
unset store_pw key_pw

(cd "$here" && ./gradlew --quiet :app:assembleRelease)

apk="$here/app/build/outputs/apk/release/app-release.apk"
[ -f "$apk" ] || { echo "no APK at $apk" >&2; exit 1; }

# Name the copy by the version Gradle stamped in, read back off the APK
# itself so the file name cannot disagree with what the phone will show.
# `aapt2` and `apksigner` ship in the SDK's build-tools; the newest one wins.
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
tools=$(ls -d "$sdk"/build-tools/*/ 2>/dev/null | sort -V | tail -1)
version="unknown"
if [ -n "$tools" ] && [ -x "$tools/aapt2" ]; then
  badging=$("$tools/aapt2" dump badging "$apk")
  code=$(sed -n "s/.*versionCode='\([^']*\)'.*/\1/p" <<< "$badging")
  name=$(sed -n "s/.*versionName='\([^']*\)'.*/\1/p" <<< "$badging")
  version="$name-$code"
  "$tools/apksigner" verify --print-certs "$apk" | grep -i 'SHA-256' | head -1
fi

mkdir -p "$apk_dir"
cp "$apk" "$apk_dir/hummingbird-$version.apk"
cp "$apk" "$apk_dir/hummingbird-latest.apk"
echo "release APK $version -> $apk_dir/hummingbird-$version.apk (and hummingbird-latest.apk)"
