#!/usr/bin/env bash
# Table-driven checks for resolve.sh's pure half, against a temp root.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd -P)"
root="$(mktemp -d)"
mkdir -p "$root/Finance/2026" "$root/House"
: >"$root/Finance/2026/receipt.pdf"
: >"$root/House/Tap washer.pdf"
: >"$root/House/fix.sh"
mkdir -p "$root/House/Tools.app"
failures=0

check() {
  local url="$1" expected="$2" actual want
  # An expected value starting "reveal: " asserts the reveal decision.
  if [[ "$expected" == "reveal: "* ]]; then want="reveal: $root/${expected#reveal: }"; else want="$root/$expected"; fi
  if actual="$(bash "$here/resolve.sh" --dry-run --root "$root" "$url" 2>/dev/null)"; then
    if [[ -z "$expected" ]]; then echo "FAIL $url -> expected refusal, got $actual"; failures=$((failures+1))
    elif [[ "$actual" != "$want" ]]; then echo "FAIL $url -> $actual"; failures=$((failures+1))
    else echo "ok   $url -> $actual"; fi
  else
    if [[ -z "$expected" ]]; then echo "ok   $url refused"
    else echo "FAIL $url refused"; failures=$((failures+1)); fi
  fi
}

check "hummingbird-open:?path=Finance%2F2026%2Freceipt.pdf" "Finance/2026/receipt.pdf"
check "hummingbird-open:?path=House%2FTap%20washer.pdf"      "House/Tap washer.pdf"
check "hummingbird-open:?path=House"                        "House"
check "hummingbird-open:?path=House%2Fa+b.pdf"              "House/a+b.pdf"
check "hummingbird-open:?path=House%2Fx%5Cny.pdf"           "House/x/ny.pdf"
check "hummingbird-open:?path=House%2Ffix.sh"               "reveal: House/fix.sh"
check "hummingbird-open:?path=House%2FTools.app"            "reveal: House/Tools.app"
check "hummingbird-open:?path="                             ""
check "hummingbird-open:?path=%2Fetc%2Fpasswd"              ""
check "hummingbird-open:?path=..%2F..%2Fetc"                ""
check "hummingbird-open:?path=a%2F..%2Fb.pdf"               ""
check "hummingbird-open:?path=C%3A%5Cx.pdf"                 ""
check "hummingbird-open:?path=~%2Fx.pdf"                    ""
check "not-ours:?path=x"                                    ""

rm -rf "$root"
if [[ $failures -gt 0 ]]; then echo "$failures failure(s)"; exit 1; else echo "all ok"; fi
