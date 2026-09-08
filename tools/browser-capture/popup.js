// The popup: DOM wiring around `lib.js`, and the one fetch. Runs as an
// extension page, which is what lets it POST cross-origin to the authority
// under `host_permissions` with no CORS lane on the server (ADR-0037).
//
// A popup is destroyed when it loses focus, so nothing here outlives a
// click: the item id is persisted *before* the fetch (`pending`), and a
// reopen on the same page inside `PENDING_TTL_MS` replays it — the route's
// idempotency by client id turns a doubled save into a 200, not a second
// item.
import {
  AUTHORITY_BASE,
  buildBody,
  canSubmit,
  classify,
  draftFromTab,
  isValidId,
  reusablePending,
  todayDeadline,
} from "./lib.js";

const api = globalThis.browser ?? globalThis.chrome;
const $ = (id) => document.getElementById(id);

document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";

function say(text, tone = "") {
  const status = $("status");
  status.textContent = text;
  status.dataset.tone = tone;
}

$("options").addEventListener("click", (event) => {
  event.preventDefault();
  api.runtime.openOptionsPage();
});

const squares = () => [...document.querySelectorAll(".square")];
const setBusy = (busy) => squares().forEach((button) => (button.disabled = busy || !canSubmit($("title").value)));

/// One capture at `destination`, the web's `submit`: Triage by default,
/// Ready for a mint, and the today square stamps the deadline over
/// whatever else — its name is a promise about the date.
async function save(token, id, draft, destination, today = false) {
  const title = $("title").value;
  if (!canSubmit(title)) return;
  setBusy(true);
  say("Saving");
  const body = buildBody({
    id,
    title,
    description: $("description").value,
    url: draft.url,
    destination,
    deadline: today ? todayDeadline(Date.now()) : undefined,
  });
  let outcome;
  try {
    const response = await fetch(`${AUTHORITY_BASE}/api/items`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    outcome = classify({
      status: response.status,
      contentType: response.headers.get("content-type"),
      bodyText: await response.text(),
      id,
    });
  } catch {
    outcome = { kind: "error", message: "Could not reach hb.twinion.net. Save again to retry." };
  }
  if (outcome.kind === "saved") {
    await api.storage.local.remove("pending");
    const landed = destination === "ready" ? "Minted into Ready" : "Added to Triage";
    say(outcome.replay ? outcome.message : landed, "done");
    setTimeout(() => window.close(), 700);
    return;
  }
  say(outcome.message, outcome.kind === "token" ? "warn" : "danger");
  if (outcome.kind === "token") $("options").hidden = false;
  setBusy(false);
}

async function main() {
  const { token, pending } = await api.storage.local.get(["token", "pending"]);
  if (!token) {
    say("No token yet. Paste one in options.", "warn");
    $("options").hidden = false;
    return;
  }
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  const draft = draftFromTab(tab);
  if (draft.problem) {
    say(draft.problem, "warn");
    return;
  }
  const now = Date.now();
  let id = reusablePending(pending, draft.url, now);
  if (id === null) {
    id = crypto.randomUUID();
    await api.storage.local.set({ pending: { id, url: draft.url, mintedAt: now } });
  }
  if (!isValidId(id)) {
    say("Could not mint an id for this capture.", "danger");
    return;
  }

  $("host").textContent = draft.host;
  $("title").value = draft.title;
  $("form").hidden = false;
  $("title").focus();
  $("title").select();

  $("title").addEventListener("input", () => setBusy(false));
  $("description").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) $("form").requestSubmit();
  });
  // Enter, and the Triage square (the form's one submit button), land in
  // Triage; the two mints are buttons of their own, never a keystroke.
  $("form").addEventListener("submit", (event) => {
    event.preventDefault();
    save(token, id, draft, "triage");
  });
  for (const button of squares()) {
    if (button.type === "submit") continue;
    button.addEventListener("click", () => save(token, id, draft, button.dataset.destination, button.dataset.today === "1"));
  }
}

main().catch((error) => say(`Something broke: ${error.message}`, "danger"));
