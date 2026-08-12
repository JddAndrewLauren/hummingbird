/**
 * The runner's client for the app-owned authority (ADR-0008), and the one
 * place in this process that holds a credential for it.
 *
 * **Why this exists at all**, when the other two ops are deliberately
 * context-blind: `microtask` writes. Its checklist has to land in the owned
 * `steps` table against a real item (#272), so this arm reads the item from
 * the authority and appends its steps there -- the same two routes
 * `.claude/skills/microtask/scripts/hb.sh` uses on the interactive arm, with
 * the same idempotency, driven from here rather than from a shell the
 * hosted model does not have (`rank-bin.js` records why it never will).
 *
 * The token is a `device`-scope token read from server-side configuration
 * (`main.js`), never from a request. Per CLAUDE.md's credential blast
 * radius: `device` is write-everything, so this module is a write
 * credential however read-only a given call looks -- which is why the
 * surface here is exactly two verbs and not a general request helper.
 *
 * `fetch` is injected for the same reason `spawn` is everywhere else: no
 * test in this suite reaches the network or needs a credential.
 */

/** Long enough for a cold Durable Object, short enough that a wedged call ends in the envelope. */
const REQUEST_TIMEOUT_MS = 15_000;

const NOT_CONFIGURED =
  "no authority token configured -- set HB_API_TOKEN as a server secret (see docs/runner.md's deploy runbook)";

/**
 * What `server.js` hands a skill when the process was started without an
 * authority token. Every call is a named failure, so an op that needs the
 * authority ends its stream in the ordinary `{ok:false}` envelope rather
 * than throwing on an undefined client -- and the two ops that do not need
 * one are untouched.
 */
export const unconfiguredAuthority = {
  async sweep() {
    return { ok: false, error: NOT_CONFIGURED };
  },
  async createStep() {
    return { ok: false, error: NOT_CONFIGURED };
  },
};

/**
 * @param {object} opts
 * @param {typeof globalThis.fetch} opts.fetch
 * @param {string} opts.baseUrl
 * @param {string} opts.token a `device`-scope authority token
 * @param {number} [opts.timeoutMs]
 * @returns {typeof unconfiguredAuthority}
 */
export function createAuthorityClient({ fetch, baseUrl, token, timeoutMs = REQUEST_TIMEOUT_MS }) {
  if (!token) return unconfiguredAuthority;

  const base = baseUrl.replace(/\/+$/, "");

  /**
   * One request. Never throws: an unreachable authority, a DNS failure and
   * a timeout are all named `{ok:false}` outcomes, because every caller
   * here is on a path that must end inside the NDJSON envelope.
   */
  async function request(method, path, body) {
    let response;
    try {
      response = await fetch(`${base}${path}`, {
        method,
        headers: {
          authorization: `Bearer ${token}`,
          ...(body === undefined ? {} : { "content-type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (err) {
      return { ok: false, error: `${method} ${path} could not reach the authority: ${err.message}` };
    }

    // Read failures get their own named outcome rather than an empty body.
    // The timeout above covers the body too, so a stall *after* the headers
    // arrive lands here -- swallowing it left that case reported as "answered
    // 200 with a non-JSON body", which names the wrong problem.
    let text;
    try {
      text = await response.text();
    } catch (err) {
      return { ok: false, error: `${method} ${path} could not read the authority's response: ${err.message}` };
    }
    return { ok: true, status: response.status, text };
  }

  function parse(method, path, { status, text }) {
    try {
      return { ok: true, value: JSON.parse(text) };
    } catch {
      // The SPA shell answers `200 text/html` on an unmatched path, so a
      // 200 is not on its own proof the API was reached -- the same trap
      // `smoke-prod.sh` and `hb.sh` both guard against.
      return { ok: false, error: `${method} ${path} answered ${status} with a non-JSON body` };
    }
  }

  return {
    /**
     * `GET /api/sweep` -- the only read of domain data there is. There is
     * no `GET /api/items/:id` and no `GET /api/steps`, so one read is the
     * whole payload and the caller filters it.
     *
     * @returns {Promise<{ok: true, sweep: {items: unknown[], steps: unknown[]}} | {ok: false, error: string}>}
     */
    async sweep() {
      const raw = await request("GET", "/api/sweep");
      if (!raw.ok) return raw;
      if (raw.status !== 200) {
        return { ok: false, error: `GET /api/sweep answered ${raw.status}: ${raw.text.slice(0, 200)}` };
      }
      const parsed = parse("GET", "/api/sweep", raw);
      if (!parsed.ok) return parsed;
      const sweep = parsed.value;
      if (sweep === null || typeof sweep !== "object" || !Array.isArray(sweep.items) || !Array.isArray(sweep.steps)) {
        return { ok: false, error: "GET /api/sweep did not answer a sweep payload" };
      }
      return { ok: true, sweep };
    },

    /**
     * `POST /api/steps`, idempotent by the client-supplied id: **201 is a
     * create and 200 is a replay**, which is what makes an interrupted or
     * repeated run safe to simply re-send (the authority returns the stored
     * row on the already-exists path -- `handlers/steps.rs`).
     *
     * @param {{id: string, item_id: string, body: string, position: number}} step
     * @returns {Promise<{ok: true, created: boolean, step: unknown} | {ok: false, error: string}>}
     */
    async createStep(step) {
      const raw = await request("POST", "/api/steps", step);
      if (!raw.ok) return raw;
      if (raw.status !== 201 && raw.status !== 200) {
        return {
          ok: false,
          error: `POST /api/steps answered ${raw.status} for "${step.body}": ${raw.text.slice(0, 200)}`,
        };
      }
      const parsed = parse("POST", "/api/steps", raw);
      if (!parsed.ok) return parsed;
      return { ok: true, created: raw.status === 201, step: parsed.value };
    },
  };
}
