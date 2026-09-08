#!/usr/bin/env bash
# hummingbird-open: handler, Mac half (ADR-0036). Maps a Dropbox-relative path
# onto this machine's Dropbox folder and opens it. See ../README.md.
#
#   resolve.sh <url>             open the resolved path (or reveal it in Finder)
#   resolve.sh --dry-run <url>   print the resolved path, open nothing; a path
#                                that would be revealed rather than opened is
#                                printed as "reveal: <path>"
#   resolve.sh --root <dir> ...  override the root (tests)
set -euo pipefail

PREFIX="hummingbird-open:?path="
dry_run=0
root=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) dry_run=1; shift ;;
    --root) root="$2"; shift 2 ;;
    *) break ;;
  esac
done
url="${1:-}"

if [[ -z "$root" ]]; then
  config="$HOME/.config/hummingbird/dropbox-root"
  if [[ -f "$config" ]]; then
    root="$(head -n1 "$config" | sed 's/[[:space:]]*$//')"
  fi
  root="${root:-$HOME/Library/CloudStorage/Dropbox}"
fi

refuse() {
  if [[ $dry_run -eq 1 ]]; then
    echo "refused: $1" >&2
  else
    # The message travels as argv, never inside the script text: a URL can
    # come from anywhere and must not be able to write AppleScript.
    osascript -e 'on run argv' \
      -e 'display alert "hummingbird could not open that file link." message (item 1 of argv)' \
      -e 'end run' "$1" >/dev/null 2>&1 || true
  fi
  exit 1
}

# RFC 3986 percent-decoding only: '+' is form-encoding and stays literal.
# Backslashes are escaped first so printf %b cannot read one as an escape.
urldecode() { local s="${1//\\/\\\\}"; s="${s//%/\\x}"; printf '%b' "$s"; }

[[ "$url" == "$PREFIX"* ]] || refuse "not a hummingbird-open URL"
relative="$(urldecode "${url#"$PREFIX"}")"
relative="$(printf '%s' "$relative" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"

[[ -n "$relative" ]] || refuse "empty path"
[[ "$relative" != /* && "$relative" != \\* ]] || refuse "absolute path refused"
[[ "$relative" != "~"* ]] || refuse "home-relative path refused"
[[ ! "$relative" =~ ^[A-Za-z]: ]] || refuse "drive letter refused"
IFS='/' read -r -a segments <<<"${relative//\\//}"
for segment in "${segments[@]}"; do
  [[ "$segment" != ".." ]] || refuse "'..' segment refused"
done

root_real="$(cd "$root" 2>/dev/null && pwd -P)" || refuse "Dropbox root not found: $root"
joined="$root_real/${relative//\\//}"
# Resolve what exists; a missing target is refused below rather than guessed at.
if [[ -e "$joined" ]]; then
  target_dir="$(cd "$(dirname "$joined")" && pwd -P)"
  target="$target_dir/$(basename "$joined")"
else
  target="$joined"
fi
[[ "$target" == "$root_real/"* ]] || refuse "path escapes the Dropbox root"

# Executable types are revealed in Finder, never opened (ADR-0036 decision 5
# as amended 2026-09-08). Decided by name alone, so a ".app" bundle counts.
action=open
case "$(printf '%s' "${target##*/}" | tr '[:upper:]' '[:lower:]')" in
  *.app|*.command|*.sh|*.pkg|*.dmg|*.scpt|*.workflow) action=reveal ;;
esac

if [[ $dry_run -eq 1 ]]; then
  if [[ $action == reveal ]]; then echo "reveal: $target"; else echo "$target"; fi
  exit 0
fi
[[ -e "$target" ]] || refuse "not found: $target"
if [[ $action == reveal ]]; then
  open -R "$target"
else
  open "$target"
fi
