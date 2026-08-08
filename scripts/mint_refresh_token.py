#!/usr/bin/env python3
"""One-time local OAuth consent flow: mint a Google refresh token.

The token carries both sweeper scopes -- Google Tasks and Gmail modify (the
capture-label drain needs to read labelled messages and remove the label).
Adding a scope later means re-running this and re-setting the secret.

Run once, on your own machine, against the Internal desktop-app OAuth client
in the twinion.net Workspace. The token it prints goes straight into
`flyctl secrets set GOOGLE_REFRESH_TOKEN=...` and nowhere else -- never a file
in this repo.

    python3 scripts/mint_refresh_token.py \
        --client-id <id> --client-secret <secret>

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
SCOPE = " ".join(
    (
        "https://www.googleapis.com/auth/tasks",
        "https://www.googleapis.com/auth/gmail.modify",
    )
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
    args = parser.parse_args()
    if not args.client_id or not args.client_secret:
        parser.error("--client-id and --client-secret (or the matching env vars) are required")

    params = {
        "client_id": args.client_id,
        "redirect_uri": REDIRECT_URI,
        "response_type": "code",
        "scope": SCOPE,
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

    print("\nGOOGLE_REFRESH_TOKEN=%s\n" % token)
    print("Store it with:\n  flyctl secrets set GOOGLE_REFRESH_TOKEN='%s'" % token)
    return 0


if __name__ == "__main__":
    sys.exit(main())
