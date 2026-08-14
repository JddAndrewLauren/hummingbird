# ADR-0022: Hummingbird's voice capture is client-local only — and Chrome 151's on-device speech surface, as measured

**Status:** accepted · 2026-08-13
**Context:** #378, the gate slice of the #377 batch that also carries #379 (the
tracer bullet), #397 (whether dictation is worth having) and the slices after
them. Docs only — no code, no schema change, no dependency change. Cites but
does not amend [ADR-0008](0008-the-authority-is-an-app-owned-server.md), whose
"the Google Tasks adapter stays for voice capture" clause governs a different
lane. Numbered 0022 because 0021 was taken by
[ADR-0021](0021-the-frontier-in-columns.md) the same day.

`SCHEMA_VERSION` does **not** move, and nothing here writes. Dictation is
pre-mutation UI state that terminates at `setDraft(...)`; it never crosses the
wasm seam, adds no field to the task model, and leaves
`CaptureBox → useCaptureWiring → Core::capture` untouched.

**This ADR does two jobs at once, deliberately.** It states a rule, and it
records the browser facts the rule was checked against — because a rule about
"only when local processing can be established" is worth nothing if nobody has
confirmed what establishing it actually looks like in a shipping browser. #377
was written against an assumed API. Part of that assumption turned out to be
wrong, and the corrections are in Decision 5.

## Decision 1 — Hummingbird-owned voice capture runs only under established local processing

**Hummingbird's built-in speech recognition is client-local only.** It uses
speech facilities supplied by the client platform, and only when the client can
*explicitly require* that recognition is performed locally. It does not bundle,
host, purchase, or proxy a speech-recognition service, and it does not fall
back to one.

**A client that cannot establish local processing offers no Hummingbird-owned
voice capture at all.** Not a degraded microphone, not a microphone with a
warning, not a microphone behind a preference. Nothing — the control is not
rendered.

**The prohibition explicitly includes error paths.** If the local recognizer
fails to start, errors mid-session, reports no match, or becomes unavailable
after having been available, the session ends and the user is told. It is never
retried against a network-backed recognizer, because an error path is precisely
where a fallback looks most reasonable and is least visible in review. There is
no configuration, build flag, or environment in which a network recognizer is
reachable from Hummingbird's own capture control.

**Why this is a decision and not a preference.** Measured on the desk machine
(Decision 5): with nothing installed, `available({langs: ["en-US"]})` — the
same call minus the `processLocally` flag — returned **`"available"`**, while
the same query *with* the flag returned `"downloadable"`. The instance property
`processLocally` **defaults to `false`**. Omitting the flag does not degrade
gracefully and does not fail — the browser reports itself **ready** for a
recognition that has not been required to stay local.

*What was measured is the readiness answer, not the audio's destination: this
session never called `start()`, so it did not observe a recognition session
reaching the network.* The inference that it would is what `processLocally`
means and why the flag exists, but it is an inference and is marked as one. It
does not need to be a measurement for Decision 1 to hold: a guarantee that
cannot be established is already unusable, whatever the browser does behind it.
The only thing standing between the default path and a Hummingbird capture is
code that asks for local processing on purpose, every time.

## Decision 2 — the absence of the capability static is `unsupported`, never an assumption

**`typeof Ctor.available === "function"` is a capability gate, not an
assertion.** Where the static is missing, the answer is `unsupported` and the
microphone does not render.

This is not defensive coding, and it is not hypothetical. A `SpeechRecognition`
constructor that has no on-device statics is a **complete, working recognizer
with no way to require local processing** — the Web Speech API has shipped in
that form for a decade, and **Safari 26.6 ships exactly it today** (measured;
Decision 6). So the failure mode of
treating a missing static as "probably fine, try it anyway" is not a crash or
an empty transcript; it is Hummingbird quietly shipping audio to a remote
service while its own ADR says it does not. The gate has to be positive:
Hummingbird offers a microphone when it has *confirmed* local processing, not
when it has failed to disprove it.

This is also why the capability is three arms — `unsupported` /
`setup-required` / `ready` — rather than a boolean, for
[ADR-0015](0015-the-standing-question-read-contract.md)'s own reason: a gap is
not an absence. `setup-required` is actionable and must say so; `unsupported`
is a browser fact nothing on the page can change and must render as nothing at
all.

## Decision 3 — future native clients inherit the rule, not an interface

**Every Hummingbird client is bound by Decision 1 as a behavioural rule it must
satisfy on its own platform.** This ADR deliberately does **not** define a
shared speech interface, a cross-client seam, or a common capability enum for
clients that do not exist yet.

**Why:** the honest scope of what is known today is one browser API on one
platform. A native client would reach a different facility with different
states, a different installation model and a different notion of what
"local" is verifiable from — and an interface designed now against a sample of
one would encode Chrome's shape as though it were the general shape. The
constraint that actually transfers is the *behaviour*: require local
processing explicitly, offer nothing when it cannot be established, never fall
back. That sentence is portable. `available()`'s status string is not.

A future client that satisfies the rule satisfies this ADR whether or not its
code resembles the web client's.

## Decision 4 — OS text-entry methods, including OS dictation, are outside the guarantee

**The guarantee covers speech recognition Hummingbird itself operates.** Text
that arrives through the platform's text-entry path — a keyboard, a paste, an
IME, the OS dictation key — is ordinary text input, indistinguishable from
typing by the time it reaches the field, and outside this ADR's scope.

This is stated rather than left implied because the omission otherwise reads as
an oversight, and because the two are easy to conflate: a user dictating into
the capture box with the OS dictation key is, visibly, dictating into
Hummingbird. Hummingbird is not the one recognizing it, has no way to
interrogate how it was recognized, and does not claim otherwise. The claim this
ADR makes is about Hummingbird's own conduct — what it builds, ships and calls
— not about every route by which characters can reach a text field.

The unattended dictated-capture lane (phone/watch/speaker → Gemini → Google
Tasks → the sweeper), which ADR-0008 keeps and `.claude/skills/parse-capture/`
governs, is likewise a different lane and is untouched by this ADR. It is not
Hummingbird-owned voice capture; it is a source the sweeper drains.

## Decision 5 — the confirmed API surface, as measured

**Measured on Google Chrome 151 (Chromium 151), macOS, on
`https://hb.twinion.net`, 2026-08-13.** These are observed values, not
documentation. Where #377's plan assumed otherwise, the assumption is corrected
here so every later slice is written against reality.

### The constructor

Exposed under **both** names, and they are the same object:

```
"SpeechRecognition" in window          → true
"webkitSpeechRecognition" in window    → true
window.SpeechRecognition === window.webkitSpeechRecognition → true
```

### The statics — one name differs from the plan

`Object.getOwnPropertyNames(SpeechRecognition)` →
`["length","name","prototype","available","install"]`

| Name | Status |
| --- | --- |
| `available` | **exists**, arity 1 |
| `install` | **exists**, arity 1 |
| `installOnDevice` | **does not exist** (`undefined`) |
| `availableOnDevice` | **does not exist** (`undefined`) |

**#377 assumed the installer was `installOnDevice({langs})`. It is
`install({langs})`.** The older origin-trial readiness form
`availableOnDevice("en-US")`, taking a bare string, does not exist at all — so
the question the issue posed as a fork has a single answer, and the
`available({langs, processLocally})` form is the only one.

### `available()` — arguments and return

Takes a `SpeechRecognitionOptions` dictionary in which **`langs` is a required
member**. Both malformed forms were exercised:

```
available("en-US")
  → TypeError: Failed to execute 'available' on 'SpeechRecognition':
    The provided value is not of type 'SpeechRecognitionOptions'.

available({ lang: "en-US", processLocally: true })      // singular key
  → TypeError: ... Failed to read the 'langs' property from
    'SpeechRecognitionOptions': Required member is undefined.
```

It returns a **string**. Three values were observed live across the install
(below): `"downloadable"`, `"downloading"` and `"available"`.

**Only those three are recorded, because only those three were seen.** #377
assumed a four-state vocabulary whose fourth member is `"unavailable"`; this
session never produced it, and this ADR does not assert it exists. Code written
against this ADR must therefore treat the returned string as **open** — match
the three known values and route everything else to `unsupported`, which is
the safe arm under Decision 2 — rather than exhaustively switching on a
four-state union it has not been shown.

A multi-language query is only as available as its weakest member:
`available({langs: ["en-US","fr-FR"], processLocally: true})` returned
`"downloadable"` while `en-US` alone returned `"available"`.

### `install()` — and the user-gesture requirement

Resolves to a **boolean**. Called from an ordinary async context while
availability was `downloadable`, it rejected:

```
NotAllowedError: Failed to execute 'install' on 'SpeechRecognition':
Requires handling a user gesture when availability is "downloadable".
```

Called from a real click handler (`navigator.userActivation.isActive === true`)
it resolved **`true`** after ~6.9 s — one sample, on one connection, so read it
as "seconds, not instant" rather than as a figure — with `available()` moving
`downloadable → downloading → available` while it ran. Once availability is
`"available"`, `install()` resolves `true` with no gesture — the requirement
applies only while the pack is still `downloadable`.

*The rejection above is the control that makes that last sentence trustworthy:
both the pre- and post-install calls were issued from the same non-gesture
evaluation context, and only the pre-install one threw. The difference is the
availability state, not the harness.*

**This promotes one of #377's design choices from preference to requirement.**
That plan chose "setup is two deliberate steps — the first mic tap in
`setup-required` explains only, and a separate control inside that hint is the
sole thing that calls the installer." That is now the *only* shape that works:
an install triggered by anything other than a direct user gesture throws. A mic
tap that silently installs is not a rejected design, it is an impossible one.

### The instance surface

`SpeechRecognition.prototype` carries, beyond the long-standing members:
`processLocally`, `phrases`, `unspokenPunctuation`, `quality`.

Defaults on a fresh instance: **`processLocally === false`**,
`quality === "command"`, `unspokenPunctuation === false`, `phrases` empty.

`processLocally` defaulting to `false` is the single most consequential fact in
this section, and Decision 1 rests on it.

*Rule 3 of [`README.md`](README.md): these tables and figures are pinned to
Chrome 151 on one machine. A later ADR that re-measures a different browser
version amends them from its own file; check this ADR's Status header.*

## Decision 6 — availability, and what is deliberately not known

| Device | Browser | Constructor | On-device statics | Verdict |
| --- | --- | --- | --- | --- |
| Desk (macOS) | Chrome 151 | both names, same object | `available`, `install` | **ready** after install |
| Desk (macOS) | Safari 26.6 | `webkitSpeechRecognition` **only** | **none** | **`unsupported`** |
| Phone | not probed | — | — | — |
| iPad | not probed (see below) | — | — | — |

### Safari ships the dangerous case, and it is now measured

Probed on `https://example.com` (a secure context with no CSP — see the
obstacle note below), Safari 26.6 on macOS:

```
"SpeechRecognition" in window        → false
"webkitSpeechRecognition" in window  → true
Object.getOwnPropertyNames(Ctor)     → ["length","name","prototype"]
available / availableOnDevice / install / installOnDevice → all undefined
"processLocally" in instance         → false
```

The prototype carries the full long-standing Web Speech surface — `start`,
`stop`, `abort`, `onresult`, `interimResults`, `maxAlternatives` — and **not
one** of `processLocally`, `phrases`, `unspokenPunctuation` or `quality`.

**This is the exact hazard Decision 2 exists for, and it is no longer
hypothetical.** Safari offers a complete, working, ready-to-use speech
recognizer that a naive capability check — "is there a constructor?" — would
happily light a microphone against. There is no flag to require local
processing, no static to ask, and therefore no way for Hummingbird to establish
the guarantee. Decision 2 routes it to `unsupported`, and the microphone does
not render. Any future change that weakens the gate to a constructor-presence
check ships cloud recognition on every Safari.

### What is still not known

**#378 asked for the phone and the iPad so that "desktop only" would be a
finding rather than an assumption. Neither was probed**, and this ADR does not
pretend otherwise — that acceptance criterion on #378 is recorded as **not
met**. The operator scoped it out on 2026-08-13.

The iPad runs the same WebKit at the same version (Safari 26.6, iPadOS 18.7),
so the Safari row above is strong evidence for it — but it is *inference from a
shared engine*, not a measurement of that device, and is not recorded as one.

**The consequence for the code is nil**, which is why the gap is tolerable:
Decision 2 routes an absent static to `unsupported`, and an unprobed device is
by definition one where local processing has not been established. The client
behaves correctly on both whatever the answer is.

### The obstacle, for whoever probes next

`hb.twinion.net` serves `script-src 'self' 'wasm-unsafe-eval'
https://accounts.google.com` with no `unsafe-inline`, and **Safari enforces CSP
against `javascript:` bookmarklets** where Chrome exempts them — so the app's
own origin cannot be probed by bookmarklet under Safari at all. Use a plain
secure origin with no CSP, or Safari Web Inspector over a cable.

Probing a different origin is sound for these statics, with one caveat worth
stating: they are gated on **secure context**, which an insecure attempt
demonstrated — one iPad run against `about:blank` reported no constructor at
all, which establishes nothing, since an insecure context is expected to
withhold the API. Origin is not wholly irrelevant, though: per #368,
`on-device-speech-recognition` is a Permissions-Policy directive gating
`install()`. It defaults to a `self` allowlist, so a **top-level** probe on any
secure origin behaves the same — but a cross-origin iframe would not, and that
directive name is itself unverified (it comes from MDN, not from this
session's measurement).

### The install, and "install once"

The install was exercised on the desk machine (above) and resolved `true`.
**Chrome was then fully quit and relaunched**, and `available()` re-queried on
a fresh tab in a fresh browser process:

```
available({ langs: ["en-US"], processLocally: true })  → "available"
available({ langs: ["en-GB"], processLocally: true })  → "available"
available({ langs: ["fr-FR"], processLocally: true })  → "downloadable"
```

**"Install once" is established, not assumed.** The surviving `fr-FR`
`"downloadable"` is the control: it shows the post-restart query is a real
per-language check rather than a blanket yes. The `en-GB` result is incidental
but worth recording — installing `en-US` also satisfied `en-GB`, so the
installed unit is evidently broader than the exact tag requested, and code
must not infer the set of installed languages from the tag it passed.

## Rejected alternatives

- **A network-backed recognizer as an error-path fallback.** The tempting
  version is narrow — "only when the local session fails to start" — and it is
  the whole hole. Measured evidence above: dropping `processLocally` is not a
  visible downgrade, it is a call that succeeds. A fallback would therefore be
  invisible in every test that asserts a transcript arrives. Rejected as the
  primary thing this ADR exists to prohibit.
- **A shared cross-client speech interface, defined now.** Rejected per
  Decision 3: designed against a sample of one, it would ship Chrome's state
  machine as though it were the general one.
- **`processLocally` as a preference rather than a requirement** — set it, and
  accept whatever the browser does. Rejected: the browser's answer to "I would
  prefer local" is not observable from the page, so the guarantee would be
  unverifiable by construction.
- **Deferring this ADR until the code exists.** Rejected on
  [ADR-0017](0017-the-standing-question-surface-axis.md)'s and
  [ADR-0021](0021-the-frontier-in-columns.md)'s shared precedent: the decision
  lands first so the slices are reviewed against a written record rather than
  a comment thread. Here it is stronger than precedent — the API names in #377
  were wrong, and every slice written before this measurement would have been
  written against them.
- **A checked-in probe page** (the superseded #362's approach) to establish the
  availability facts. Rejected by #377: this slice already opens the same
  browser, and `docs/SURFACES.md` records what landing code with no caller
  cost the S10–S13 batch. The probe ran as a throwaway snippet in the console
  and its output lives here.

## Out of scope

- **Whether dictation is worth having at all.** That is a preference, it needs
  a working microphone in the real capture box, and it is **#397** — taken
  against the #379 tracer bullet, before the rest of the feature is built.
  Nothing in this ADR argues that Hummingbird should have voice capture; it
  constrains what voice capture may be *if* it ships.
- **The UI, the session lifecycle, the splice semantics and the cancel
  gesture.** Those are #379's, decided in #377.
- **Audio storage or retention.** Hummingbird stores no audio; recognition
  terminates at `setDraft(...)` and the transcript becomes ordinary draft text.
  There is nothing to decide, which is why there is no decision here.

## Consequences

- **#379 must be written against `install()`, not `installOnDevice()`**, and
  against a required-`langs` dictionary. Any slice already drafted with the
  assumed names needs correcting before it is implemented.
- **The installer must be reachable only from a user gesture.** #377's
  two-step setup is now load-bearing; a reviewer should treat any install call
  not rooted in a click handler as a defect.
- **CSP needs no change, and that remains a consequence rather than a
  coincidence.** On-device recognition issues no fetch from the page, so CSP
  would never catch a relaxation to the cloud path — the browser-internal
  network call is invisible to it. The `processLocally` requirement is the
  only enforcement point there is, which is why Decision 1 states it as
  absolute.
- **A code comment or module header claiming "falls back to cloud if
  unavailable" is a defect against this ADR**, not a nit, whether or not the
  code does it.
- **Safari renders no microphone, and that is the correct output, not a bug.**
  #383's design pass and #384's real-browser acceptance should expect the
  capture box to look exactly as it does today under Safari — the `unsupported`
  arm renders nothing, which is also what keeps the existing
  `CapturePopover.test.tsx` cases untouched. A bug report of "the mic is
  missing in Safari" is this ADR working.
- **Superseding this ADR requires a measurement, not an argument.** A proposal
  to relax Decision 1 must show the browser state it was measured in, in the
  form Decision 5 uses.

### Tripwire

**Re-measure and amend when any of these becomes true:** `available` or
`install` is renamed again or moves off the constructor; `processLocally`
changes its default away from `false`; `available()` returns a value outside
the three observed above; **Safari gains any on-device static**, which would
move it out of `unsupported` and make the microphone render somewhere it never
has; or the phone or iPad is probed, closing the last gap in Decision 6 and
letting the feature's device scope finally be stated as a finding rather than
the part-measurement it is today.
