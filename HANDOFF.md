# Handoff — prove the item door on hardware, then land #141

**Written:** 2026-08-17, from the session that implemented ADR-0027's
item-detail slice (slices A–G2 of the plan, plus a wrap-up fix).
**Repo:** `JddAndrewLauren/hummingbird`. **Branch:** `now-page-handoff`,
based on `origin/main` at `a6c04c0`, ten commits ahead
(`3ba53bb`…`ec1e000`).

## The goal

Two things, in order: **run the hardware checks** that are the only evidence
this lane works, then **open the PR to main** closing **#141** and **#518**.

Nothing on this branch has ever rendered on a device. Every gate that could
run locally has (server clippy + tests + the wasm32 worker build, client
clippy + tests, `:app:testDebugUnitTest` + `:app:assembleDebug`), and none of
them can see a pixel or a push.

## What shipped, in one paragraph

A tapped `item-threshold/v1` notification now opens the **item** rather than
the alert about it. `PushNotification` carries `source`/`source_key`;
`hummingbird_core::decisions::notification_tap_target` turns those two
strings into a destination with no mirror read; `Core::item_detail` assembles
the record (project, checklist, open blockers, the live alert) and
`Core::act_acking_alert` makes completing or cancelling an item ack the alert
about it; the seam maps it to `ItemDetailRecord` and exposes `edit_item`;
Android gets a second deep-link door and an `ItemDetailScreen` with an edit
mode. **Read ADR-0027 first** — it is the whole design, and this handoff does
not restate it.

## Before anything: read these, in this order

1. `docs/adr/0027-an-alert-opens-the-thing-it-is-about.md` — the decision.
2. `client/android/README.md` § "Proving the lane on hardware" — the
   eighteen checks. **1–12 passed on 2026-08-17 (#517); 13–18 have never
   been run.**
3. `client/core/src/item_detail.rs` — what each field of the record decides.

## The work

### 1. The hardware run (checks 13–18)

The trap that will cost you time if nobody says it: **an `item-threshold/v1`
alert cannot be raised with `POST /api/alerts`.** M2's recipe used that
endpoint; this source is minted by the Durable Object's alarm sweep over
items with near deadlines (`server/authority/src/sweep.rs:200`). So give an
item a deadline inside the sweep threshold and let the alarm ring. Check 17
(the degrade path) is the one that still uses `POST /api/alerts`, and it
needs an **ingest**-scope token — a `device` token gets a 403 by design.

The server half must be deployed first (`a2bd086` widens the payload), and
the client build installed on the Fold. An old server with a new client is
safe — it degrades to alert detail, which is the permanent contract — so
deploying is not strictly ordered before installing, but you will not see
check 13 pass until both are live.

**When it goes silent, `wrangler tail` is the first move, always.** The FCM
lane fails silently by construction; a 201 carries no delivery information.
That cost about 40 minutes on 2026-08-17 to a bad `FCM_SERVICE_ACCOUNT`
value.

Screenshots need `adb exec-out screencap -p -d <display-id>` — without `-d`
adb writes a warning banner into the PNG, and the ids differ inner vs cover.

### 2. What to watch for that the tests cannot see

- **The Ack from item detail goes through the core's mutation queue**, not
  `AckWorker`. The authority reads stale until that queue flushes; give it a
  moment before calling check 15 failed.
- **Check 16 is the interesting one.** Completing the item should drain
  **two** queue entries, the `act` first, and the alert should leave live
  without a second gesture. If only one drains, the composition is not
  wired the way `completing_an_item_acks_its_alert_act_first` claims.
- **A behaviour change to a shipped surface**: the seam's `act` now routes
  through `Core::act_acking_alert`, so **the Now list's checkmark also
  silences a ringing item-threshold alert**. That is deliberate
  (`CONTEXT.md`'s amended **Ack** makes it a property of the gesture, not
  the screen) but it is worth seeing once on the device before it lands on
  `main`. If it feels wrong, the revert is one line in
  `client/ffi-mobile/src/lib.rs` plus a second seam method.
- **The item screen has never been looked at.** There is no visual gate for
  Android at all (`docs/SURFACES.md` § "Surface: native Android" now says
  so explicitly). Nobody has seen the edit mode's layout, the axes line, or
  the discard dialog.

### 3. The PR

Against `main`, closing **#141** and **#518**. `git fetch` and re-check
branch state first — other sessions share this repo. #518's fix is already
on this branch at `cc113cb`; the issue stayed open pending this merge.

## The one open question

**Does an archived item keep its checklist?** `Core::item_detail` reads
`steps` through `mirror.steps_for_item`, which is presence-live only. Whether
an archived item's steps survive in the mirror depends on what the
authority's sweep sends for rows belonging to an archived item — not chased
this session. If they do not, history renders with an empty checklist and no
explanation, which is a quiet lie rather than a crash.

Decide it before, or during, check 18 (the archived-item check) — that check
is exactly where it would show. If steps do vanish, the honest fix is
probably a record field saying so rather than silently rendering nothing.

## Traps that still apply

- Never `cargo fmt --all` from `client/` — a path dependency drags in all of
  `server/`, which is not rustfmt-clean. `fmt` is **not** a CI gate here;
  match rustfmt's shape by hand for your own lines.
- `client/android` CI is `android.yml` and it path-filters. A new top-level
  directory would match no filter and merge green with zero checks.
- Other sessions share this repo: `git fetch` and re-check branch/PR state
  before merging.

## Related, out of scope

- **#519** — `RegistrationOutcome.DONE` reports success, no-token and
  `Unauthorized` identically. Diagnosability only; independent of this.
- **#128** (second launcher icon), **#129** (Wear OS tile) — additive
  surfaces, neither closes #141.
