// What to tell someone whose Connect/Reconnect attempt failed — a pure copy
// module, following `shell/status-label.ts`. Every string the calendar
// connection can put on screen is decided here and tested here; the wiring
// and the screen only carry it.
//
// The voice is the product's (design README): state what is true and stop, no
// reassurance, no apology. Each answer has two parts, because an error the
// reader cannot act on is just bad news — `message` says what happened,
// `hint` says what to do about it.

export interface ConnectErrorCopy {
  message: string;
  hint: string;
}

/** `standalone` is whether this is an installed home-screen app rather than a
 * browser tab. It changes the advice, not the diagnosis: an installed iOS app
 * has a storage container separate from Safari's, so "try it in Safari" is
 * actively wrong there — the connection made in a tab would not be the one
 * the app can see. */
export function connectErrorCopy(error: string, standalone: boolean): ConnectErrorCopy {
  switch (error) {
    case "token_request_timed_out":
      return {
        message: "Google never answered.",
        hint: standalone
          ? "The sign-in window may have opened outside the app. Close it and try again from here."
          : "Check the connection and try again.",
      };
    case "popup_failed_to_open":
      return {
        message: "The Google sign-in window did not open.",
        hint: "A pop-up blocker is the usual cause. Allow pop-ups for this site and try again.",
      };
    case "popup_closed":
      return {
        message: "The Google sign-in window closed before it finished.",
        hint: "Try again and complete the sign-in.",
      };
    case "access_denied":
      return {
        message: "Google declined the request.",
        hint: "Calendar access has to be granted for the read-only scope. Try again and accept it.",
      };
    case "gis_script_load_failed":
      return {
        message: "Google's sign-in script did not load.",
        hint: "This device may be offline or blocking accounts.google.com. Try again when it is not.",
      };
    case "gis_unavailable":
      return {
        message: "Google's sign-in script loaded but exposed nothing to call.",
        hint: "Reload the app and try again.",
      };
    case "gis_request_failed":
      return {
        message: "The sign-in request could not be started.",
        hint: "Reload the app and try again.",
      };
    case "no_access_token":
      return {
        message: "Google answered without a token.",
        hint: "Try again. If it repeats, remove this app's calendar access in your Google account and connect again.",
      };
    case "state_mismatch":
      return {
        message: "The sign-in answer did not match the request this app made.",
        hint: "Press Connect from this screen and finish the sign-in without leaving it. If this page was opened by following a link from somewhere else, close it and open the app directly.",
      };
    case "no_expiry":
      return {
        message: "Google returned a token without saying how long it lasts.",
        hint: "Try again. If it repeats, remove this app's calendar access in your Google account and connect again.",
      };
    default:
      // The unknown case echoes the raw code rather than swallowing it, and a
      // reader who can quote the code is one who can be helped; "something
      // went wrong" is not.
      //
      // Note what the wording above this line has to be careful about. The
      // error space is NOT only Google's: GIS adds `error_callback` types over
      // time, but `google/redirect-flow.ts` mints codes of its own —
      // `state_mismatch` (this app's CSRF check failing) and `no_expiry` (this
      // app refusing an unusable token). Both are cased above precisely so
      // they stop arriving here, where the sentence would have said Google
      // reported them and sent the reader off to debug the wrong system. An
      // unknown code keeps the attribution deliberately vague for the same
      // reason: at this point nothing knows whose code it is.
      return {
        message: `The connection failed with "${error}".`,
        hint: "Try again. The code above is what to search for if it repeats.",
      };
  }
}
