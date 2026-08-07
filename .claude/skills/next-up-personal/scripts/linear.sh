#!/usr/bin/env bash
# Linear helper for the /next-up-personal skill.
# Usage: linear.sh survey
#        linear.sh get <IDENT>
#        linear.sh move <IDENT> <state-name>
#        linear.sh comment <IDENT> <file>
#        linear.sh unlabel <IDENT> <label>
set -euo pipefail

TEAM_KEY="${LINEAR_TEAM_KEY:-ION}"
DUE_SOON_DAYS=7
KEY_PATH="${LINEAR_API_KEY_PATH:-$HOME/.config/linear/api-key}"
if [[ ! -f "$KEY_PATH" ]]; then
  echo "linear.sh: no Linear API key at $KEY_PATH — create one at https://linear.app/settings/account/security and save it to that path" >&2
  exit 1
fi
KEY=$(<"$KEY_PATH")
ENDPOINT="https://api.linear.app/graphql"

# Linear returns HTTP 200 with an "errors" array on failure — inspect, never assume.
gql() { # $1 = JSON payload; prints the response body on success
  local resp
  resp=$(curl -sS "$ENDPOINT" -H "Authorization: $KEY" -H "Content-Type: application/json" -d "$1")
  if [[ $(jq 'has("errors")' <<<"$resp") == true ]]; then
    jq -r '"linear.sh: GraphQL error: " + (.errors | map(.message) | join("; "))' <<<"$resp" >&2
    exit 1
  fi
  printf '%s' "$resp"
}

issue_uuid() { # $1 = identifier
  gql "$(jq -n --arg id "$1" \
    '{query:"query($id:String!){issue(id:$id){id}}",variables:{id:$id}}')" \
    | jq -er '.data.issue.id'
}

# An issue is blocked when something *blocks it* — that edge lives in inverseRelations,
# never in relations. Getting the direction backwards inverts the frontier.
BLOCKER_FRAGMENT='inverseRelations{nodes{type issue{identifier state{name type}}}}'

cmd="${1:-}"
case "$cmd" in
  survey)
    q='query($team:String!){
      issues(first:250, filter:{team:{key:{eq:$team}}}){
        pageInfo{hasNextPage}
        nodes{
          identifier title url priority priorityLabel dueDate createdAt
          state{name type} project{name} labels{nodes{name}}
          '"$BLOCKER_FRAGMENT"'
        }
      }
      projects(first:50){ pageInfo{hasNextPage} nodes{name content} }
    }'
    payload=$(jq -n --arg q "$q" --arg team "$TEAM_KEY" '{query:$q,variables:{team:$team}}')
    gql "$payload" | jq --arg today "$(date +%F)" --argjson dueSoonDays "$DUE_SOON_DAYS" '
      # completed / canceled / duplicate are all "shut" — for issues and for blockers alike.
      def shut($t): $t=="completed" or $t=="canceled" or $t=="duplicate";
      def dayEpoch($d): ($d + "T00:00:00Z" | fromdateiso8601);
      def dueSoon:
        .dueDate != null
        and .dueDate > $today
        and dayEpoch(.dueDate) <= (dayEpoch($today) + ($dueSoonDays * 86400));
      def rec: {
        identifier, title, url, priority, priorityLabel, dueDate, createdAt,
        state: .state.name,
        project: (.project.name // null),
        labels: [.labels.nodes[].name],
        agent: ([.labels.nodes[].name] | index("agent") != null),
        overdue: (.dueDate != null and .dueDate < $today),
        dueToday: (.dueDate == $today),
        dueSoon: dueSoon,
        blockers: .openBlockers
      };
      def fog($content):
        if (($content // "") | length) == 0 then null
        else
          ($content | split("\n")) as $L
          | (reduce range(0; $L|length) as $i ({inside:false, out:[]};
              $L[$i] as $ln
              | if   ($ln | test("^##[ \t]+Fog[ \t]*$"; "i")) then .inside = true
                elif (.inside and ($ln | test("^##[ \t]"))) then .inside = false
                elif .inside then .out += [$ln]
                else . end)).out
          | join("\n") | sub("^\\s+";"") | sub("\\s+$";"")
          | if . == "" then null else . end
        end;

      .data as $d
      | $d.issues.nodes as $all
      | [ $all[]
          | select(shut(.state.type) | not)
          | . + {openBlockers: [ .inverseRelations.nodes[]
                                 | select(.type == "blocks")
                                 | select(shut(.issue.state.type) | not)
                                 | .issue.identifier ]} ] as $open
      # Externally Blocked work is not actionable. Otherwise include the normal frontier,
      # plus overdue and due-soon work from non-frontier states.
      | [ $open[]
          | select(.state.name != "Blocked")
          | select(.state.name == "Ready"
                   or .state.name == "In Progress"
                   or (.dueDate != null and .dueDate <= $today)
                   or dueSoon) ] as $cand
      | {
          today: $today,
          truncated: ($d.issues.pageInfo.hasNextPage or $d.projects.pageInfo.hasNextPage),
          candidates: [ $cand[] | select(.openBlockers | length == 0) | rec ],
          blocked:    [ $cand[] | select(.openBlockers | length  > 0) | rec ],
          health: {
            triage:   ([ $all[] | select(.state.name == "Triage")   ] | length),
            grilling: ([ $all[] | select(.state.name == "Grilling") ] | length)
          },
          projects: [ $d.projects.nodes[]
            | . as $p
            | [ $all[] | select(.project.name == $p.name) ] as $pi
            | { name: $p.name,
                actionsTotal: ($pi | length),
                actionsOpen: ([ $pi[] | select(shut(.state.type) | not) ] | length),
                fog: fog($p.content) } ]
        }'
    ;;
  get)
    ident="${2:?usage: linear.sh get <IDENT>}"
    q='query($id:String!){issue(id:$id){
        id identifier title description url priority priorityLabel dueDate createdAt
        state{name type} project{name content} labels{nodes{name}}
        '"$BLOCKER_FRAGMENT"'
      }}'
    payload=$(jq -n --arg q "$q" --arg id "$ident" '{query:$q,variables:{id:$id}}')
    gql "$payload" | jq '
      def shut($t): $t=="completed" or $t=="canceled" or $t=="duplicate";
      .data.issue
      | . + {blockers: [ .inverseRelations.nodes[]
                         | select(.type == "blocks")
                         | select(shut(.issue.state.type) | not)
                         | .issue.identifier ]}
      | del(.inverseRelations)'
    ;;
  move)
    ident="${2:?usage: linear.sh move <IDENT> <state-name>}"
    state="${3:?usage: linear.sh move <IDENT> <state-name>}"
    info=$(gql "$(jq -n --arg id "$ident" \
      '{query:"query($id:String!){issue(id:$id){id team{states{nodes{id name}}}}}",variables:{id:$id}}')")
    uuid=$(jq -er '.data.issue.id' <<<"$info")
    # Match by state NAME — Blocked/In Progress and Ready/Grilling share types.
    sid=$(jq -er --arg n "$state" \
      'first(.data.issue.team.states.nodes[] | select(.name == $n) | .id) // empty' <<<"$info") || true
    if [[ -z "${sid:-}" ]]; then
      echo "linear.sh: no state named '$state' on ${ident}'s team (known: $(jq -r '[.data.issue.team.states.nodes[].name]|join(", ")' <<<"$info"))" >&2
      exit 1
    fi
    payload=$(jq -n --arg id "$uuid" --arg s "$sid" \
      '{query:"mutation($id:String!,$s:String!){issueUpdate(id:$id,input:{stateId:$s}){success}}",variables:{id:$id,s:$s}}')
    gql "$payload" | jq -e '.data.issueUpdate.success' >/dev/null
    echo "ok"
    ;;
  comment)
    ident="${2:?usage: linear.sh comment <IDENT> <file>}"
    file="${3:?usage: linear.sh comment <IDENT> <file>}"
    uuid=$(issue_uuid "$ident")
    payload=$(jq -n --arg id "$uuid" --rawfile body "$file" \
      '{query:"mutation($id:String!,$body:String!){commentCreate(input:{issueId:$id,body:$body}){success}}",variables:{id:$id,body:$body}}')
    gql "$payload" | jq -e '.data.commentCreate.success' >/dev/null
    echo "ok"
    ;;
  unlabel)
    ident="${2:?usage: linear.sh unlabel <IDENT> <label>}"
    label="${3:?usage: linear.sh unlabel <IDENT> <label>}"
    info=$(gql "$(jq -n --arg id "$ident" \
      '{query:"query($id:String!){issue(id:$id){id labels{nodes{id name}}}}",variables:{id:$id}}')")
    uuid=$(jq -er '.data.issue.id' <<<"$info")
    lid=$(jq -er --arg n "$label" \
      'first(.data.issue.labels.nodes[] | select(.name == $n) | .id) // empty' <<<"$info") || true
    if [[ -z "${lid:-}" ]]; then
      echo "ok (no '$label' label on $ident)"   # idempotent: removing an absent label is not an error
      exit 0
    fi
    payload=$(jq -n --arg id "$uuid" --arg l "$lid" \
      '{query:"mutation($id:String!,$l:String!){issueRemoveLabel(id:$id,labelId:$l){success}}",variables:{id:$id,l:$l}}')
    gql "$payload" | jq -e '.data.issueRemoveLabel.success' >/dev/null
    echo "ok"
    ;;
  *)
    cat >&2 <<'USAGE'
usage: linear.sh survey
       linear.sh get <IDENT>
       linear.sh move <IDENT> <state-name>
       linear.sh comment <IDENT> <file>
       linear.sh unlabel <IDENT> <label>
USAGE
    exit 2
    ;;
esac
