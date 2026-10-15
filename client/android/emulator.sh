#!/bin/bash
# Boot an AVD, install this checkout's debug APK on it, open the app, and
# stay in the foreground until the emulator exits: the last `start` step of
# the `phone-live` rig (docs/agents/rigs.yaml). Run `:app:assembleDebug`
# first; this script installs what that left behind and builds nothing.
#
#   ./client/android/emulator.sh <avd> <console-port>
#
# The console port fixes the adb serial (`emulator-<port>`), so the rig's
# ready probe names exactly this emulator and never a second one the
# operator has open. A debug install on an AVD never touches the phone's
# release install (deploy.sh's header says why those two must not mix).
# The AVD's data partition, and so a device token pasted into Settings,
# survives the restart; `-no-snapshot-save` only skips the quick-boot image.
set -euo pipefail

avd=${1:?usage: emulator.sh <avd> <console-port>}
port=${2:?usage: emulator.sh <avd> <console-port>}
here=$(cd "$(dirname "$0")" && pwd)
apk="$here/app/build/outputs/apk/debug/app-debug.apk"
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
serial="emulator-$port"
adb="$sdk/platform-tools/adb"

[ -f "$apk" ] || { echo "no APK at $apk; run :app:assembleDebug first" >&2; exit 1; }

"$sdk/emulator/emulator" -avd "$avd" -port "$port" -no-snapshot-save -no-boot-anim &
emu=$!

# No `adb wait-for-device`: it never returns when the emulator dies before
# registering (the AVD already open elsewhere), so this loop does the waiting.
until [ "$("$adb" -s "$serial" shell getprop sys.boot_completed 2> /dev/null | tr -d '\r')" = 1 ]; do
  kill -0 "$emu" 2> /dev/null || { echo "emulator exited before boot completed" >&2; exit 1; }
  sleep 2
done

"$adb" -s "$serial" install -r "$apk"
"$adb" -s "$serial" shell am start -n net.twinion.hummingbird/.MainActivity
wait "$emu"
