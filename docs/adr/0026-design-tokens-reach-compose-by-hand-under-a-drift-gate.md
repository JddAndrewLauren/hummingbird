# ADR-0026: Design tokens reach Compose by hand, under a drift gate

**Status:** accepted · 2026-08-14
**Context:** the design-sync session that pulled `ui_kits/android/` into the
mirror (PR #476), executing the operator handoff from the M0 build on
[#141](https://github.com/JddAndrewLauren/hummingbird/issues/141). The M0
skeleton shipped hand-written `Color.kt`/`Theme.kt`/`Type.kt`; the handoff
named "how CSS custom properties become Compose theme values" as a decision
to make deliberately rather than inherit. Decided through a written tradeoff
analysis, operator-confirmed. Extends
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md)'s
line — rendering stays per-client, written natively — down into the token
layer; amends nothing.

## The decision

**Design tokens are hand-ported into the Compose theme files, and the
mechanical layer of that port is guarded by a CI drift gate (#483) rather
than generated.** There is no Android equivalent of the web client's copy
step: the web consumes `tokens/*.css` verbatim because CSS is its native
tongue, and Android consumes the same tokens as idiomatic Kotlin because
that is *its* native tongue. The mirror at
`.claude/skills/hummingbird-design/tokens/` is the single source both read
from — one by copy, one by port.

## The two layers, and why only one is checkable

M0's theme files already drew the load-bearing line, and this ADR makes it
policy:

- **`Color.kt` is the mechanical layer**: named constants whose values are
  verbatim from `tokens/colors.css`, under its header's rule "never a value
  the CSS doesn't have". Every CSS-derived literal belongs here — #483 moves
  the few that M0 left inline in `Theme.kt` — so that "a value in `Color.kt`"
  and "a value that claims a CSS identity" are the same set.
- **`Theme.kt` and `Type.kt` are the judgment layer**: mapping Hummingbird's
  semantic tokens onto Material 3's *different* slot vocabulary
  (`--surface-quiet` → `surfaceVariant`, scrim alpha applied at use), px→sp
  conversions, font-family substitutions. No generator can produce this from
  CSS, whatever tooling is bought — the slots don't exist in the source.

The drift gate (#483) is a JVM unit test that parses the mirrored
`colors.css` and asserts every `Color.kt` constant matches its token, with
the CSS as the authority; a constant with no token mapping fails, which is
the mechanical form of M0's rule. `android.yml` gains the mirror's
`tokens/**` path so the gate runs on exactly the PRs that change tokens —
the path-filtered-CI trap that workflow's own header describes.

## Rejected alternatives

**Codegen (CSS → Kotlin in a Gradle task).** The automatable surface is the
mechanical layer only — about 26 constants at M0, ~237 lines of CSS in
total — and the token set has not changed since it was authored
(2026-08-09). A parser-generator with its own tests would be longer than the
file it generates, would still leave the judgment layer unprotected (which
is where the interesting mistakes live), and buys its convenience at a
cadence that doesn't exist. Verification gives the same drift guarantee at a
fraction of the standing infrastructure.

**Hand-port with discipline by review alone.** The M0 status quo. Rejected
because the failure mode is invisible at exactly the moment it happens: a
token change arrives via mirror re-pull, the web picks it up by copy, and
nothing on a phone looks "broken" — the brand is just quietly stale. Review
catches what a reviewer thinks to compare; the gate compares everything,
every run.

## The recorded exit

If M1 styling turns the palette into something actively iterated — roughly,
the Android token surface growing past the full ramps (~100+ constants), or
re-porting recurring more than about monthly — the drift test's mapping
table becomes the maintenance burden and a generator for the mechanical
layer earns its keep. That switch is an amendment to this ADR, not a silent
drift; the judgment layer stays hand-written either way.
