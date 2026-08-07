#!/usr/bin/env bash
# Linear helper for the /microtask skill.
# Usage: linear.sh get <IDENT> | linear.sh set-description <IDENT> <file>
set -euo pipefail

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

cmd="${1:-}"
case "$cmd" in
  get)
    ident="${2:?usage: linear.sh get <IDENT>}"
    payload=$(jq -n --arg id "$ident" \
      '{query:"query($id:String!){issue(id:$id){id identifier title description url project{name} labels{nodes{name}}}}",variables:{id:$id}}')
    gql "$payload" | jq '.data.issue'
    ;;
  set-description)
    ident="${2:?usage: linear.sh set-description <IDENT> <file>}"
    file="${3:?usage: linear.sh set-description <IDENT> <file>}"
    uuid=$(gql "$(jq -n --arg id "$ident" \
      '{query:"query($id:String!){issue(id:$id){id}}",variables:{id:$id}}')" | jq -r '.data.issue.id')
    payload=$(jq -n --arg id "$uuid" --rawfile desc "$file" \
      '{query:"mutation($id:String!,$desc:String!){issueUpdate(id:$id,input:{description:$desc}){success}}",variables:{id:$id,desc:$desc}}')
    gql "$payload" | jq -e '.data.issueUpdate.success' >/dev/null
    echo "ok"
    ;;
  *)
    echo "usage: linear.sh get <IDENT> | linear.sh set-description <IDENT> <file>" >&2
    exit 2
    ;;
esac
