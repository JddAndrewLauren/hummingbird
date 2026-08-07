# Linear API reference for /next-up-personal

All calls are GraphQL POSTs to `https://api.linear.app/graphql`, wrapped by
`scripts/linear.sh`. Read this when the script needs changing — the skill itself never
hand-writes a query at runtime.

## Auth

```bash
KEY=$(cat ~/.config/linear/api-key)
curl -sS https://api.linear.app/graphql \
  -H "Authorization: $KEY" \
  -H "Content-Type: application/json" \
  -d '{"query":"..."}'
```

The key goes in the raw `Authorization` header — **no `Bearer` prefix**. Linear answers
HTTP 200 with an `errors` array on failure, so every response is inspected, never assumed.

Overrides for testing: `LINEAR_API_KEY_PATH`, `LINEAR_TEAM_KEY` (default `ION`).

## Team vocabulary (org `twinion`, team `ION`)

Resolved at runtime — never hardcode ids (team `ION` is currently
`84ab9e0b-f455-42d7-a48a-49e65da3b2e6`, but the script looks it up by key).

States and their **types**:

| State | Type |
| --- | --- |
| Triage | `triage` |
| Grilling | `unstarted` |
| Backlog | `backlog` |
| Ready | `unstarted` |
| **Blocked** | **`started`** |
| In Progress | `started` |
| Done | `completed` |
| Canceled | `canceled` |
| Duplicate | `duplicate` |

**Filter by state *name*, never by type alone.** Blocked shares `started` with In
Progress, and Ready shares `unstarted` with Grilling. Type is only used the other way
round — to decide whether something is *shut* (`completed` / `canceled` / `duplicate`).

Labels (all exist in the workspace, including `agent`): group `energy` → `low` / `high`;
group `size` → `quick` / `medium` / `deep`; ungrouped contexts `@home`, `@office`,
`@errands`, `@computer`, `@calls`, `@out`; ungrouped `agent`.

**Priority is Linear-native and inverted:** `0` = No priority, `1` = Urgent, `2` = High,
`3` = Medium, `4` = Low. A numeric sort is wrong twice over — bigger is *less* urgent, and
`0` means "unset", not "most urgent". The survey emits `priorityLabel` alongside the number;
rank on the label.

## Blocked-by direction (load-bearing)

An issue is blocked when its **`inverseRelations`** contains a node of `type: "blocks"`
whose `issue` (the blocker) is not shut. The forward `relations` list is the opposite —
things this issue blocks. Getting this backwards inverts the frontier.

Confirmed live on `ION-16`: `inverseRelations` lists ION-14 and ION-15 (its blockers);
`relations` lists ION-18 and ION-19 (what it blocks).

`inverseRelations` also carries `related` and `duplicate` edges — filter to `blocks`.

## The survey query

One call, two root fields:

```graphql
query($team: String!) {
  issues(first: 250, filter: {team: {key: {eq: $team}}}) {
    pageInfo { hasNextPage }
    nodes {
      identifier title url priority priorityLabel dueDate createdAt
      state { name type } project { name } labels { nodes { name } }
      inverseRelations { nodes { type issue { identifier state { name type } } } }
    }
  }
  projects(first: 50) { pageInfo { hasNextPage } nodes { name content } }
}
```

All team issues are fetched (shut ones included) because the project action counts need
the denominator. `hasNextPage` on either root surfaces as `truncated: true`.

Watch the complexity budget: Linear caps queries at complexity 10000, and nesting
`issues` under `projects` at these page sizes blows past it (~57k). Project membership is
derived from each issue's `project.name` instead.

### Output shape

```json
{
  "today": "2026-08-07",
  "truncated": false,
  "candidates": [{
    "identifier": "ION-14", "title": "…", "url": "…",
    "priority": 0, "priorityLabel": "No priority",
    "dueDate": null, "createdAt": "…",
    "state": "Ready", "project": "Update Acumatica",
    "labels": ["@computer", "quick", "low"],
    "agent": false, "overdue": false, "dueToday": false, "dueSoon": false,
    "blockers": []
  }],
  "blocked":  [{ "…": "same shape", "blockers": ["ION-15", "ION-14"] }],
  "health":   { "triage": 3, "grilling": 0 },
  "projects": [{ "name": "…", "actionsTotal": 8, "actionsOpen": 6, "fog": "…verbatim or null" }]
}
```

`candidates` = Ready + In Progress + anything overdue, due today, or due within the next
seven calendar days in any non-shut state, minus the mechanical exclusions: externally
`Blocked` issues and issues with an open `blocked by` relation. The relation-blocked ones
that were dropped land in `blocked`, so the skill can explain a short list.

`fog` is the **verbatim** text of the project's `## Fog` section from the project's
`content` field (`description` is a separate 255-char summary and never holds the Route),
or `null` when there's no content or no such section. It is not interpreted — a Fog
section reading `* None — …` is a non-null string, and judging that is the skill's job.

## Delegation mutations

```graphql
# state — resolve the id by NAME off the issue's own team
mutation($id:String!,$s:String!){ issueUpdate(id:$id,input:{stateId:$s}){ success } }

mutation($id:String!,$body:String!){ commentCreate(input:{issueId:$id,body:$body}){ success } }

# label removal takes top-level args, not an input object
mutation($id:String!,$l:String!){ issueRemoveLabel(id:$id,labelId:$l){ success } }
```

`unlabel` is idempotent: an absent label reports ok rather than failing, so a re-run of a
half-finished delegation doesn't error out.

These four writes — state, comment, label removal — are the **only** mutations this skill
is allowed to make. It never mints, never edits a description, never touches the Route.
