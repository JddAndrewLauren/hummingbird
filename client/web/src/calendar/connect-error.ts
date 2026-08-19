// What to tell someone whose Connect/Reconnect attempt failed — a pure copy
// module, following `shell/status-label.ts`. Every string the calendar
// connection can put on screen is decided here and tested here; the wiring
// and the screen only carry it.
//
// The voice is the product's (design README): state what is true and stop, no
// reassurance, no apology. Each answer has two parts, because an error the
// reader cannot act on is just bad news — `message` says what happened,
// `hint` says what to do about it.
//
// **#584 deleted the interactive browser OAuth surface**, and with it every
// code that could only come from a popup, GIS's script, or this app's own
// redirect handling (a CSRF `state` mismatch, a token with no usable
// expiry). There is also no `standalone` parameter any more — that existed
// only to change the advice for the redirect flow's dead end on an
// installed iOS app, and every attempt now, silent or interactive, is the
// same same-origin POST regardless of where it runs.
//
// The whole error space left is `calendar/authority-token-client.ts`'s
// seven codes (its own header lists and explains them); this switch has one
// case per code, plus a fallback for anything nobody has classified yet.

export interface ConnectErrorCopy {
  message: string;
  hint: string;
}

export function connectErrorCopy(error: string): ConnectErrorCopy {
  switch (error) {
    case "no_device_token":
      return {
        message: "This device has no token stored.",
        hint: "Enter a device token below, then try again.",
      };
    case "authority_rejected_device_token":
      return {
        message: "The stored device token was rejected.",
        hint: "Enter a fresh device token below and try again.",
      };
    case "authority_unconfigured":
      return {
        message: "The server has no Google calendar credentials configured.",
        hint: "Nothing to try from here — tell the operator to check the Google calendar credential.",
      };
    case "authority_upstream":
      return {
        message: "Google declined the server's request for a calendar token.",
        hint: "Try again later. If it repeats, ask the operator to check the Google calendar credential.",
      };
    case "authority_unreachable":
      return {
        message: "The server never answered.",
        hint: "Check the connection and try again.",
      };
    case "bad_token_response":
      return {
        message: "The server's answer could not be read.",
        hint: "Try again.",
      };
    case "no_access_token":
      return {
        message: "The server answered without a token.",
        // Not "remove this app's calendar access in your Google account":
        // under ADR-0028 there is no per-device grant to revoke — the only
        // grant is the server-held dedicated refresh token, shared by every
        // device — and this code is the authority's own 200 body lacking a
        // usable token (`authority-token-client.ts`), a server-side fault,
        // not something Google is declining per-reader. The fix is on the
        // server, so the hint points there.
        hint: "Try again. If it repeats, this is a server-side fault — tell the operator to check the Google calendar credential.",
      };
    default:
      // The unknown case echoes the raw code rather than swallowing it, and a
      // reader who can quote the code is one who can be helped; "something
      // went wrong" is not.
      return {
        message: `The connection failed with "${error}".`,
        hint: "Try again. The code above is what to search for if it repeats.",
      };
  }
}
