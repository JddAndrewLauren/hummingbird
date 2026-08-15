# UI kit — Android

Three Material 3 screens: **Now** (context tile, today's list, extended FAB),
**Alerts** (the notification lane with the Ack gesture, in dark mode) and the
**Capture sheet** (bottom sheet over a scrim, with optional energy/size sliders and a context dropdown).

The bezel, status bar and gesture nav come from `android-frame.jsx`;
everything inside is Hummingbird. The FAB is the only place the brand orange
appears as a large fill on Android.

No Android client exists in the repository yet — these screens are proposed,
built from `CONTEXT.md` and the ADRs.
