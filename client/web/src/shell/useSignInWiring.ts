import { useEffect, useRef, useState } from "react";
import type { StorageLike } from "../screens/storage";
import type { CoreStatus } from "../store/store";
import { signInWithGoogle, type WorkerLike } from "../store/worker-client";
import {
  beginGoogleSignIn,
  deviceNameFromUserAgent,
  GOOGLE_SIGNIN_CLIENT_ID,
  signInFailureMessage,
  type SignInReturn,
} from "../task/google-signin";
import type { TaskTokenSubmitOutcome } from "../task/token";

// Signing in with Google on the web (#840, ADR-0040 decisions 1 and 3):
// starting the redirect, and exchanging what the `/signin` return carried
// for this browser's device token. `main.tsx` reads the return off the URL
// once, before the first render, and hands it here; this hook waits for the
// core, trades the ID token through `Core::sign_in_with_google` as platform
// `web`, and hands a minted token to the token wiring's own submit — the
// same door a pasted one takes, so it is stored and pushed exactly as one.
// Nothing downstream knows the token came from a sign-in.

/** What the device-token card says about a sign-in. */
export type SignInUiState =
  | { kind: "idle" }
  | { kind: "pending" }
  | { kind: "error"; message: string };

export interface SignInWiringDeps {
  /** The `hummingbird-signin` Web client id, or `null` on a build without
   * one — then there is no button, only the paste form. */
  clientId: string | null;
  /** Where the pending sign-in's nonce waits: `sessionStorage`. */
  storage: StorageLike;
  origin: string;
  navigate: (url: string) => void;
  userAgent: string;
  now: () => number;
}

export interface SignInWiring {
  signIn: SignInUiState;
  /** Leaves for Google's sign-in page, or `null` when no client is
   * configured. */
  beginSignIn: (() => void) | null;
}

export function useSignInWiring(
  worker: WorkerLike,
  status: CoreStatus,
  signInReturn: SignInReturn | null,
  submitToken: (input: string) => Promise<TaskTokenSubmitOutcome>,
  deps: SignInWiringDeps = browserDeps(),
): SignInWiring {
  const [signIn, setSignIn] = useState<SignInUiState>(() =>
    signInReturn === null
      ? { kind: "idle" }
      : signInReturn.kind === "error"
        ? { kind: "error", message: signInReturn.message }
        : { kind: "pending" },
  );
  // One return, one exchange — a ref, so StrictMode's second effect pass
  // and every later re-render find it already spent. Never cancelled on
  // unmount: a token the authority minted and this view then ignored would
  // be a signed-in device row with no device behind it.
  const exchanged = useRef(false);

  useEffect(() => {
    if (status !== "ready" || signInReturn?.kind !== "token" || exchanged.current) {
      return;
    }
    exchanged.current = true;
    void (async () => {
      const answer = await signInWithGoogle(
        worker,
        signInReturn.idToken,
        deviceNameFromUserAgent(deps.userAgent),
        deps.now(),
      );
      if (answer.kind !== "signed_in" || answer.token === null) {
        setSignIn({
          kind: "error",
          message: signInFailureMessage(answer.kind === "signed_in" ? "failed" : answer.kind, answer.error),
        });
        return;
      }
      const outcome = await submitToken(answer.token);
      setSignIn(
        outcome === "ok"
          ? { kind: "idle" }
          : { kind: "error", message: "Signed in, but this device could not save its token. Sign in again." },
      );
    })();
    // `deps` and `submitToken` are read once, at the exchange.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status, signInReturn, worker]);

  const { clientId } = deps;
  const beginSignIn =
    clientId === null
      ? null
      : () => deps.navigate(beginGoogleSignIn(deps.storage, clientId, deps.origin));

  return { signIn, beginSignIn };
}

function browserDeps(): SignInWiringDeps {
  return {
    clientId: GOOGLE_SIGNIN_CLIENT_ID,
    storage: sessionStorage,
    origin: window.location.origin,
    navigate: (url) => window.location.assign(url),
    userAgent: navigator.userAgent,
    now: Date.now,
  };
}
