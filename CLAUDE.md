# hummingbird

Personal GTD-style task system on Linear (org `twinion`, team `ION`). See `CONTEXT.md`
for the domain glossary.

## The sweeper

`sweep.py` is the one-way Google Tasks → Linear Triage sweeper — the only built
artifact of v0 capture. Stdlib-only Python, one-shot, fired every 15 minutes by
supercronic in a Fly worker. Its invariants (frozen `NAMESPACE`, Linear-first
ordering, no Actions `schedule:`, no `[http_service]`) are load-bearing and
decided upstream; read `docs/sweeper.md` before touching any of them.

## Agent skills

### Issue tracker

Issues live as GitHub issues in `JddAndrewLauren/hummingbird`, driven via the `gh` CLI;
the wayfinder map is issue #1. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary — `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`,
`wontfix`, plus the non-triage `plan` role. All exist on the tracker.
See `docs/agents/triage-labels.md`.

### Microtasking

`/microtask <issue-id>` breaks one already-selected, stalled Linear issue into a checklist of
~2–5-minute Steps written into its body. See `.claude/skills/microtask/SKILL.md`.

### Domain docs

Single-context — root `CONTEXT.md` glossary plus `docs/adr/`.
See `docs/agents/domain.md`.
