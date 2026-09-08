#!/usr/bin/env bash
# Builds "Hummingbird Open.app" beside this script and registers it with
# LaunchServices as the hummingbird-open: handler (ADR-0036).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd -P)"
app="$here/Hummingbird Open.app"
resolve="$here/resolve.sh"
chmod +x "$resolve"

rm -rf "$app"
osacompile -o "$app" "$here/open.applescript"
# The applet reads resolve.sh's path from this file at run time; writing it
# verbatim avoids escaping the path for sed and for an AppleScript literal.
printf '%s\n' "$resolve" >"$app/Contents/Resources/resolve.path"

plist="$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier net.twinion.hummingbird.open" "$plist" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string net.twinion.hummingbird.open" "$plist"
/usr/libexec/PlistBuddy -c "Delete :CFBundleURLTypes" "$plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes array" "$plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes:0 dict" "$plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes:0:CFBundleURLName string hummingbird open" "$plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes array" "$plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string hummingbird-open" "$plist"

# Register with LaunchServices (opening once also does it; -R is the explicit form).
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
echo "Built and registered: $app"
echo "Try: open 'hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf'"
