#!/usr/bin/env python3
"""One-time local OAuth consent flow: mint a Google refresh token.

By default the token carries three scopes, and that one is shared by every
server-side Google consumer in the repo (operator decision on #486): Google
Tasks and Gmail modify for the sweeper (the capture-label drain needs to read
labelled messages and remove the label), plus Calendar readonly for
`calendar-poll`. `gmail-poll` reads through the same credential rather than a
narrower dedicated one -- one consent, one secret to rotate. Adding a scope
later means re-running this and re-setting the secret in all of its places
(Fly, and the GitHub Actions secrets both pollers read).

**Why `--scope` exists (#581).** A refresh_token grant returns an access token
bearing the *whole* grant: Google ignores a `scope` parameter on that exchange,
so a caller cannot down-scope at mint time. That is fine while every consumer
is server-side, and it stops being fine the moment a token is handed to a
browser -- ADR-0028's `POST /api/google/calendar_token` does exactly that, so
reusing the shared credential there would mean a stolen `device` token also
yields a bearer that can modify the operator's Gmail. The remedy is a second,
dedicated credential minted against its own OAuth client with one scope, which
is what `--scope` is for. Prefer the default; reach for the override only when
the consumer is a different blast radius, and say which one in the item's
1Password notes.

Run once, on your own machine, against the Internal desktop-app OAuth client
in the twinion.net Workspace. The token it prints goes into
`flyctl secrets set GOOGLE_REFRESH_TOKEN=...` and `gh secret set
GOOGLE_REFRESH_TOKEN`, and nowhere else -- never a file in this repo. A
`--scope` run goes wherever its own consumer reads it (for #581's calendar
credential: `wrangler secret put` on `hummingbird-authority`, three secrets,
from the operator's terminal and never Actions -- CLAUDE.md's blast-radius
rule).

    python3 scripts/mint_refresh_token.py \
        --client-id <id> --client-secret <secret>

    python3 scripts/mint_refresh_token.py \
        --client-id <id> --client-secret <secret> \
        --scope https://www.googleapis.com/auth/calendar.readonly

(or set GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET in the environment)

Add http://localhost:8765/ to the OAuth client's authorized redirect URIs, or
use a Desktop app client, which accepts loopback redirects on any port.
"""

import argparse
import http.server
import json
import os
import sys
import urllib.parse
import urllib.request
import webbrowser

AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth"
TOKEN_URL = "https://oauth2.googleapis.com/token"
DEFAULT_SCOPES = (
    "https://www.googleapis.com/auth/tasks",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar.readonly",
)
PORT = 8765
REDIRECT_URI = "http://localhost:%d/" % PORT

_result = {}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        query = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        _result.update({k: v[0] for k, v in query.items()})
        body = b"Consent received. You can close this tab and return to the terminal."
        if "error" in _result:
            body = ("Authorization failed: %s" % _result["error"]).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):  # keep the console clean
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client-id", default=os.environ.get("GOOGLE_CLIENT_ID"))
    parser.add_argument("--client-secret", default=os.environ.get("GOOGLE_CLIENT_SECRET"))
    parser.add_argument(
        "--scope",
        action="append",
        metavar="URL",
        help=(
            "request this scope instead of the default three; repeat for more than "
            "one. See the module header for when a dedicated credential is the "
            "right call. Default: " + " ".join(DEFAULT_SCOPES)
        ),
    )
    args = parser.parse_args()
    if not args.client_id or not args.client_secret:
        parser.error("--client-id and --client-secret (or the matching env vars) are required")

    scopes = args.scope or list(DEFAULT_SCOPES)
    print("Requesting: %s\n" % " ".join(scopes))

    params = {
        "client_id": args.client_id,
        "redirect_uri": REDIRECT_URI,
        "response_type": "code",
        "scope": " ".join(scopes),
        "access_type": "offline",
        # Force the consent screen: without it Google reissues an access token
        # only, and no refresh token comes back on a repeat authorization.
        "prompt": "consent",
    }
    url = AUTH_URL + "?" + urllib.parse.urlencode(params)
    print("Open this URL and grant access as the twinion.net account:\n\n%s\n" % url)
    webbrowser.open(url)

    server = http.server.HTTPServer(("127.0.0.1", PORT), Handler)
    print("Waiting for the redirect on %s ..." % REDIRECT_URI)
    while not _result:
        server.handle_request()
    server.server_close()

    if "code" not in _result:
        print("Authorization failed: %s" % _result, file=sys.stderr)
        return 1

    body = urllib.parse.urlencode(
        {
            "code": _result["code"],
            "client_id": args.client_id,
            "client_secret": args.client_secret,
            "redirect_uri": REDIRECT_URI,
            "grant_type": "authorization_code",
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        TOKEN_URL,
        data=body,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.loads(response.read())

    token = payload.get("refresh_token")
    if not token:
        print("No refresh_token in the response: %s" % payload, file=sys.stderr)
        return 1

    granted = payload.get("scope", "(none reported)")
    print("\nGranted scope: %s" % granted)
    print("\nGOOGLE_REFRESH_TOKEN=%s\n" % token)
    if args.scope:
        # A --scope run is a dedicated credential, so its destinations are its
        # consumer's, not the shared token's. Naming them here would be a guess.
        print("Store it in 1Password first (dev vault, `hummingbird`-prefixed title),")
        print("then set it wherever its own consumer reads it, from this terminal.")
    else:
        print("Store it in 1Password first, then in both places that read it:")
        print("  flyctl secrets set GOOGLE_REFRESH_TOKEN='%s' --app hummingbird-sweeper" % token)
        print("  gh secret set GOOGLE_REFRESH_TOKEN   # gmail-poll + calendar-poll")
    return 0


if __name__ == "__main__":
    sys.exit(main())
