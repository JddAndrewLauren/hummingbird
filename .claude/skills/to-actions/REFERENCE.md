# Authority API reference for /to-actions

Everything goes through `scripts/hb.sh`, which speaks the app-owned authority
(ADR-0008/0009) at `hb.twinion.net/api/*`. This file is what the script's verbs
mean and what the vocabulary is; read `SKILL.md` for the interview and the rules.

## Auth

One bearer token from `~/.config/hummingbird/api-token`, `device` scope. The
script reads it and puts it in a header — **never on a command line**, where
`ps` and shell history would both see it. Override the path with
`HB_API_TOKEN_PATH` and the host with `HB_API_BASE` (which is how the
`wrangler dev` round-trip is run).

`device` is the authority's only read-capable scope and it is
write-everything, so a token that can survey can also mint. That is the same
posture `server/scripts/smoke-prod.sh` records, and the same reason neither
ever goes into GitHub Actions.

## Reads are the whole sweep

There is no `GET /api/items/:id` and no per-table listing. `GET /api/sweep` is
the only read of domain data, and it carries every table: `items`, `projects`,
`routes`, `fog`, `steps`, `blocked_by`, `settings`, `alerts`,
`context_snapshots`, `rules`. So every read verb here fetches once and filters
in jq, and the script caches the payload for the life of one invocation.

This is why the old GraphQL query-cost findings are gone rather than ported:
there is no cost model here, no page size to tune and no `hasNextPage` to
check. One call, whole state.

## `HB-<seq>` is a client-side affordance

No route accepts or resolves it. `seq` is server-minted at create and appears
only in `Item.seq` and the web UI's render. `hb.sh` maps `HB-42` onto its uuid
off the sweep it has already fetched, and passes a bare uuid through untouched.

## Vocabulary

The owned schema's own spellings, and they are closed — the authority rejects
anything else with a 400.

- **`stage`**: `triage`, `grilling`, `ready` (the minting target), `in_progress`,
  `blocked` (external waits only), `done`.
  **There is no Backlog, no Canceled and no Duplicate stage.** Cancelling is
  `archived_at`, which is what `Core::act`'s own cancel sets.
- **`size`**: `quick`, `short`, `deep`. (Not `medium` — that was Linear's word.)
- **`energy`**: `low`, `medium`, `high`. (`medium` is new; there were two.)
- **`context`**: free text, by convention `@home`, `@office`, `@computer`,
  `@calls`, `@errands`, `@out`. Not a closed vocabulary and not a label — one
  string column.
- **`priority`**: `0..=4`, and it **kept Linear's inverted, holed encoding**
  (`client/web/src/screens/priority.ts` says so). `0` means *unset*, not
  lowest. Never sort or reason on the raw number; the "no priority" case is a
  real value, not a gap.
- **`agent`**: a boolean column, CONTEXT.md's **delegation axis**. Never
  proposed by default — delegation is deliberate.

There are **no labels and no label groups**. The owned schema dissolved them
into these typed columns, which is why size/energy/context/agent are set
directly on a create rather than resolved to label ids first.

## Find or create the project

```
hb.sh project-find "<name>"     # substring, case-insensitive; every match
hb.sh project-create "<name>"   # only after an explicit yes
```

`project-find` answers with the whole Route per match: the project row, its
`route` row, open and resolved `fog`, its actions in `project_pos` order, and
the live `blocked_by` edges into those actions.

**Creating a project creates its Route row.** The 1:1 invariant is structural
and there is deliberately no `POST /api/routes` — `route-set` patches a row
that already exists.

Both creates are **idempotent by client-supplied id**, and `hb.sh` derives that
id deterministically from the name, so re-running `project-create` returns the
stored row rather than minting a second project.

## The Route is four records, not four markdown sections

The old four-section template in the project description is **gone**. Routes
being first-class records is one of the two things that triggered ADR-0008, so
this is the change, not a detail of it:

| Was | Is |
| --- | --- |
| `## Destination` | `routes.destination` |
| `## Notes` | `routes.notes` |
| `## Fog` | `fog` rows, each with `question`, `position`, `resolved_at` |
| `## Actions` | `items` with `project_id`, ordered by `project_pos` |

```
hb.sh route-set <project-ref> [--destination <file>] [--notes <file>]
                              [--clear-destination] [--clear-notes]
hb.sh fog-add <project-ref> "<question>"
hb.sh fog-resolve <fog-id>
hb.sh archive <ref>
```

An **absent** flag leaves the field untouched, `--clear-*` nulls it, and a file
sets it — the same absent/null/value distinction `RoutePatch`'s own
double-`Option` carries. There is no "rewrite the whole Route" call and no need
for one: nothing is a blob any more, so an edit to Notes cannot lose Fog.

**An open fog row *is* fog.** The Linear-era rule that "the fog check is a
reading, not a regex" was about parsing a markdown section that might say
"None — the unknowns are carried inside the two investigation actions"; a row
with a `resolved_at` needs no such reading. Resolve a fog question when it is
answered; don't write a row that says there is no fog.

## Mint an action

```
hb.sh mint <manifest-file>
```

The manifest is a JSON array of `CreateItem` objects:

```jsonc
[
  {"title": "Dig out the current policy document",
   "project_id": "<uuid>", "project_pos": 1,
   "stage": "ready", "size": "quick", "energy": "low", "context": "@home"}
]
```

**Every id is settled before the first write.** `mint` derives any id the
manifest does not supply (deterministically, from the project and the title)
and only then starts posting — so a batch interrupted halfway is replayed by
simply re-running it: creates are idempotent by client id, the already-minted
half answers `200` with the stored row, and nothing is duplicated. That is what
lets the whole batch be one confirmed pass with no bookkeeping between halves
of it.

Server-stamped fields (`seq`, `created_at`, `updated_at`, `version`) cannot be
supplied — the authority answers `400`, not a silent no-op, and so does a typo.
Every minted action is normalized to `stage: ready`; an explicit non-ready
stage or any `agent` field is rejected because delegation belongs to
`/next-up-hb`.

## Sequencing: `blocked_by` edges

```
hb.sh block <ref> <blocker-ref>     # <ref> is blocked by <blocker-ref>
```

`POST /api/blocked_by {item_id, blocker_id}` reads exactly as written. **There
is no direction to get backwards.** Linear's inverted `issueRelationCreate`
recipe — where "A is blocked by B" had to be created as "B blocks A" — and the
standing warning that reversing it silently inverts the frontier are both
deleted, not ported.

The edge's identity is the pair, so the create is idempotent by construction,
and a re-add clears `removed_at` on the same row.

**Never set `stage: blocked` for an inter-action dependency.** That stage means
an external wait (a callback, a part in the mail) and nothing else.

## Cancel-and-remint

The owned schema has no Canceled stage, so cancelling is `archived_at`. Use the
helper seam rather than hand-writing the request:

```
hb.sh archive <ref>
```

The underlying request is:

```
PATCH /api/items/<uuid> {"expected_version": N, "archived_at": <ms epoch>}
```

Linear's **Duplicate-state quirk is deleted outright** — there is no such state
and no relation to mint first.

Rows are never deleted, only flagged (ADR-0003), so an archived action stays
readable in the Ledger. There is no DELETE anywhere in this API; anything you
write, you live with.

## Writes are CAS

Read `version` from the sweep, `PATCH` with `expected_version`, and on a `409`
compare every touched field with the original row. `hb.sh` retries once only
when the changes are disjoint, accepts a current row whose touched values
already equal the request, and stops on a divergent touched field. A second
conflict is still bounded failure.

A patch whose submitted values already match the stored ones is a no-op: `200`,
no version bump, so no peer is made to re-pull an unchanged row.
