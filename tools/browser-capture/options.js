// The options page: one masked field that stores the device token in
// `storage.local` (never `sync` — a token is per profile, per machine, and
// revocable alone). The shape check is the whole validation; the authority
// is the only thing that can say whether the token is live.
import { tokenProblem } from "./lib.js";

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

refresh().catch((error) => say(`Something broke: ${error.message}`, "danger"));
