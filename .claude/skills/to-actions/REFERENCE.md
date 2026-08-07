# Linear API reference for /to-actions

All calls are GraphQL POSTs to `https://api.linear.app/graphql`.

## Auth

```bash
KEY=$(cat ~/.config/linear/api-key)
curl -s https://api.linear.app/graphql \
  -H "Authorization: $KEY" \
  -H "Content-Type: application/json" \
  -d '{"query":"..."}'
```

The key goes in the raw `Authorization` header — **no `Bearer` prefix**.

## Team vocabulary (org `twinion`, team `ION`)

Resolve IDs at runtime — don't hardcode them:

```graphql
{ teams(filter: {key: {eq: "ION"}}) { nodes {
    id
    labels { nodes { id name parent { name } } }
    states { nodes { id name type } }
} } }
```

- **States**: Triage, Grilling, Backlog, **Ready** (minting target), Blocked
  (external waits only), In Progress, Done, Canceled, Duplicate.
- **Label groups**: `size` → `quick` / `medium` / `deep`; `energy` → `low` / `high`.
- **Context labels**: `@home`, `@office`, `@computer`, `@calls`, `@errands`, `@out`.
- **`agent`**: exists; never proposed by default.

## Find or create the project

```graphql
{ projects(filter: {name: {containsIgnoreCase: "<name>"}}) {
    nodes { id name description } } }
```

Create (only after an explicit yes):

```graphql
mutation { projectCreate(input: {name: "<name>", teamIds: ["<teamId>"]}) {
    project { id } } }
```

## Mint an action

```graphql
mutation { issueCreate(input: {
    teamId: "<teamId>", projectId: "<projectId>",
    title: "<title>", stateId: "<readyStateId>",
    labelIds: ["<contextLabelId>", "<sizeLabelId>", "<energyLabelId>"]
}) { issue { id identifier } } }
```

## Sequencing: blocked-by relations

"A is blocked by B" is created as **B blocks A**:

```graphql
mutation { issueRelationCreate(input: {
    issueId: "<B.id>", relatedIssueId: "<A.id>", type: blocks
}) { issueRelation { id } } }
```

Never move an issue to the Blocked state to represent this.

## Cancel-and-remint

Cancel = move the issue to the Canceled state; its relations die with it. Then mint
replacements fresh:

```graphql
mutation { issueUpdate(id: "<issueId>", input: {stateId: "<canceledStateId>"}) {
    success } }
```

## Refresh the Route

```graphql
mutation { projectUpdate(id: "<projectId>", input: {description: "<markdown>"}) {
    success } }
```

Fetch the current description first and rebuild the full four-section template —
`projectUpdate` replaces the whole description.
