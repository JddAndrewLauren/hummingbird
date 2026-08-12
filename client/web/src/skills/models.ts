// The models the Rewrite gesture can ask for (#273).
//
// **#274 deletes this file.** That issue puts a real backend picker in
// front of the seam, and whatever it learns the available models from —
// a `/api/skills/backends` route, a remembered preference — replaces this
// constant wholesale. Anything more here now would make #274 a migration
// instead of an addition.
//
// Two rules keep the list honest, and both are load-bearing:
//
// 1. **The stamp never reads this file.** What the outcome renders comes
//    from the envelope (`run-state.ts`'s `stampLabel`), so if the runner
//    runs something other than what was asked for, the disagreement is
//    visible rather than papered over by this list.
// 2. **The select renders only when there is more than one entry**, gated
//    on `CLOUD_RUNNER_MODELS.length` rather than at the render site — so a
//    list trimmed to just the default silently tells no lie instead of
//    offering a choice that isn't one.
//
// The ids are the Claude CLI's own aliases, not model ids invented here.
// The runner validates whatever is sent against a charset rule rather than
// an allowlist (`runner/src/claude-cli.js`), so this list constrains only
// what the app *offers* — it is not the runner's contract.

export interface CloudRunnerModel {
  value: string;
  label: string;
}

/** The empty value means "send no `model` arg at all" — see
 * `microtask-args.ts`, which omits the key — so the runner's own configured
 * default stays the default rather than being named twice. */
export const CLOUD_RUNNER_MODELS: CloudRunnerModel[] = [
  { value: "", label: "Default model" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "haiku", label: "Haiku" },
];
