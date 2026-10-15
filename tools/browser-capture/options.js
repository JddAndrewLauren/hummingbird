// The options page: Sign in with Google first (ADR-0040 decision 3), a
// masked paste field behind "Paste a token instead" as the fallback. Either
// way the device token lands in `storage.local` under `token` (never `sync`
// — a token is per profile, per machine, and revocable alone), so the popup
// cannot tell how it got there. Every decision — the auth URL, the nonce,
// the fragment, the name, what the authority's answer means — is in
// `lib.js`; this file is the wiring and the two calls (`launchWebAuthFlow`,
// the sign-in `POST`).
import {
  AUTHORITY_BASE,
  SIGNIN_PATH,
  authUrl,
  canSubmit,
  defaultDeviceName,
  newNonce,
  parseSigninRedirect,
  signinBody,
  signinOutcome,
  tokenProblem,
} from "./lib.js";

const api = globalThis.browser ?? globalThis.chrome;
const $ = (id) => document.getElementById(id);

document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";

function say(text, tone = "") {
  const status = $("status");
  status.textContent = text;
  status.dataset.tone = tone;
}

async function refresh() {
  const { token } = await api.storage.local.get("token");
  $("state").textContent = token ? "stored" : "none";
  $("forget").hidden = !token;
}

function showPaste() {
  $("form").hidden = false;
  $("paste").hidden = true;
  $("token").focus();
}

async function signIn(name) {
  const nonce = newNonce();
  let redirectedTo;
  try {
    redirectedTo = await api.identity.launchWebAuthFlow({
      url: authUrl({ redirectUri: api.identity.getRedirectURL(), nonce }),
      interactive: true,
    });
  } catch (error) {
    // Closing the Google window lands here too.
    say(`Sign-in did not finish: ${error.message}`, "warn");
    return;
  }
  const answer = parseSigninRedirect(redirectedTo, nonce);
  if (answer.problem) {
    say(answer.problem, "danger");
    return;
  }
  let outcome;
  try {
    const response = await fetch(`${AUTHORITY_BASE}${SIGNIN_PATH}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(signinBody({ idToken: answer.idToken, name })),
    });
    outcome = signinOutcome({
      status: response.status,
      contentType: response.headers.get("content-type"),
      bodyText: await response.text(),
    });
  } catch {
    outcome = { kind: "error", message: "Could not reach hb.twinion.net. Nothing was stored." };
  }
  if (outcome.kind !== "signed_in") {
    say(outcome.message, outcome.kind === "rate_limited" || outcome.kind === "unavailable" ? "warn" : "danger");
    if (outcome.kind === "unconfigured") showPaste();
    return;
  }
  await api.storage.local.set({ token: outcome.token });
  say(outcome.message, "done");
  await refresh();
}

$("signin").addEventListener("submit", async (event) => {
  event.preventDefault();
  const name = $("name").value;
  if (!canSubmit(name)) {
    say("Name this device first.", "danger");
    return;
  }
  $("google").disabled = true;
  try {
    await signIn(name);
  } finally {
    $("google").disabled = false;
  }
});

$("paste").addEventListener("click", showPaste);

$("form").addEventListener("submit", async (event) => {
  event.preventDefault();
  // `trim` takes the `\r\n` a Windows clipboard appends; anything else of
  // the wrong shape is refused rather than stored to fail as a 401 later.
  const token = $("token").value.trim();
  const problem = tokenProblem(token);
  if (problem) {
    say(problem, "danger");
    return;
  }
  await api.storage.local.set({ token });
  $("token").value = "";
  say("Stored.", "done");
  refresh();
});

$("forget").addEventListener("click", async () => {
  await api.storage.local.remove(["token", "pending"]);
  say("Forgotten. Revoke it on the authority too if it is being rotated.");
  refresh();
});

async function main() {
  const { os } = await api.runtime.getPlatformInfo();
  $("name").value = defaultDeviceName(os);
  await refresh();
}

main().catch((error) => say(`Something broke: ${error.message}`, "danger"));
