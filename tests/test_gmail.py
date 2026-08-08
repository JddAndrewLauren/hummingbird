"""Cred-free tests for the Gmail capture adapter and the adapter seam.

Same approach as test_sweep.py: everything network-shaped goes through
sweep.http_json, so these tests monkeypatch exactly that one function and
assert on the calls it receives.
"""

import contextlib
import io
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import sweep  # noqa: E402

CFG = sweep.Config(
    google_client_id="cid",
    google_client_secret="secret",
    google_refresh_token="refresh",
    linear_api_key="lin_key",
    healthcheck_url="https://hc.example/tasks",
    denylist_path="/nonexistent/denylist.json",
    gmail_healthcheck_url="https://hc.example/gmail",
)

LABEL_ID = "Label_7"
LABELS = {
    "labels": [
        {"id": "INBOX", "name": "INBOX"},
        {"id": LABEL_ID, "name": sweep.GMAIL_CAPTURE_LABEL},
    ]
}

# 1600000000s = 2020-09-13T12:26:40Z, a fixed point for the Date line.
INTERNAL_DATE = "1600000000000"


def message(
    msg_id="msg-1",
    thread_id="thr-1",
    subject="Fwd: the thing",
    sender="Jane Doe <jane@example.com>",
    snippet="do the thing",
    internal_date=INTERNAL_DATE,
    label_ids=(LABEL_ID, "INBOX"),
):
    headers = []
    if subject is not None:
        headers.append({"name": "Subject", "value": subject})
    if sender is not None:
        headers.append({"name": "From", "value": sender})
    payload = {
        "id": msg_id,
        "threadId": thread_id,
        "snippet": snippet,
        "labelIds": list(label_ids),
        "payload": {"headers": headers},
    }
    if internal_date is not None:
        payload["internalDate"] = internal_date
    return payload


class GmailNamespaceTest(unittest.TestCase):
    # Frozen vector. If this fails, GMAIL_NAMESPACE or the derivation changed,
    # and every issue id the Gmail adapter has ever minted just moved -- which
    # means every still-open capture would be recreated as a duplicate.
    VECTOR = ("18f2a4b3c9d0e1f2", "792a3e18-647c-4824-a5a6-48f0fb44f4db")

    def test_frozen_vector(self):
        message_id, expected = self.VECTOR
        self.assertEqual(
            sweep.deterministic_v4(message_id, sweep.GMAIL_NAMESPACE), expected
        )

    def test_namespaces_stay_disjoint(self):
        # The same raw key in the two sources must never mint the same issue.
        key = self.VECTOR[0]
        self.assertNotEqual(
            sweep.deterministic_v4(key, sweep.GMAIL_NAMESPACE),
            sweep.deterministic_v4(key, sweep.NAMESPACE),
        )


class GmailDeriveCaptureTest(unittest.TestCase):
    DESCRIPTION_HEAD = (
        "From: Jane Doe <jane@example.com>\n"
        "Date: 2020-09-13T12:26:40Z\n"
        "Thread: https://mail.google.com/mail/u/0/#all/thr-1"
    )

    def test_subject_maps_to_title_verbatim_with_stable_description(self):
        title, description = sweep.gmail_derive_capture(message())
        self.assertEqual(title, "Fwd: the thing")
        self.assertEqual(description, self.DESCRIPTION_HEAD + "\n\ndo the thing")

    def test_encoded_subject_and_sender_are_decoded(self):
        title, description = sweep.gmail_derive_capture(
            message(
                subject="=?utf-8?q?R=C3=A9sum=C3=A9?=",
                sender="=?utf-8?q?Ren=C3=A9?= <rene@example.com>",
            )
        )
        self.assertEqual(title, "Résumé")
        self.assertIn("From: René <rene@example.com>", description)

    def test_blank_subject_falls_back_to_first_snippet_line(self):
        title, _ = sweep.gmail_derive_capture(
            message(subject="   ", snippet="\n  ring the bank  \nabout the card")
        )
        self.assertEqual(title, "ring the bank")

    def test_missing_subject_header_falls_back_too(self):
        title, _ = sweep.gmail_derive_capture(message(subject=None, snippet="hello"))
        self.assertEqual(title, "hello")

    def test_fully_blank_message_still_captures_as_no_subject(self):
        # A labelled message earned capture by the human gesture, however
        # blank it looks -- nothing is ever silently lost.
        title, description = sweep.gmail_derive_capture(message(subject="", snippet=""))
        self.assertEqual(title, "(no subject)")
        # No snippet block, but the stable head survives as the road back.
        self.assertEqual(description, self.DESCRIPTION_HEAD)

    def test_snippet_html_entities_are_unescaped(self):
        title, description = sweep.gmail_derive_capture(
            message(subject="", snippet="Don&#39;t forget the thing")
        )
        self.assertEqual(title, "Don't forget the thing")
        self.assertTrue(description.endswith("\n\nDon't forget the thing"))

    def test_missing_internal_date_falls_back_without_crashing(self):
        _, description = sweep.gmail_derive_capture(message(internal_date=None))
        self.assertIn("Date: (unknown time)", description)


class FakeHttp:
    """Records every call; answers Google Tasks, Gmail, and Linear reads."""

    def __init__(self, linear_responses=None, labels=None, messages=None,
                 lists_response=None):
        self.calls = []
        self.linear_responses = list(linear_responses or [])
        self.labels = labels if labels is not None else LABELS
        self.messages = messages if messages is not None else [message()]
        self.lists_response = lists_response or {
            "items": [{"id": "list-1", "title": "My Tasks"}]
        }
        self.tasks = {"items": [{"id": "task-1", "title": "call the vet", "notes": ""}]}

    def __call__(self, url, method="GET", headers=None, body=None):
        self.calls.append({"url": url, "method": method, "headers": headers, "body": body})
        if url.startswith("https://hc.example/"):
            return {}
        if url == sweep.GOOGLE_TOKEN_URL:
            return {"access_token": "at"}
        if url == sweep.LINEAR_URL:
            if self.linear_responses:
                return self.linear_responses.pop(0)
            return {"data": {"issueCreate": {"success": True, "issue": {"id": "x"}}}}
        if "/users/@me/lists" in url:
            return dict(self.lists_response)
        if "/tasks?" in url:
            return dict(self.tasks)
        if url.startswith(sweep.TASKS_URL) and method == "PATCH":
            return {"status": "completed"}
        if "/users/me/labels" in url:
            return dict(self.labels)
        if "/users/me/messages?" in url:
            return {"messages": [{"id": m["id"]} for m in self.messages]}
        if "/modify" in url and method == "POST":
            return {}
        for msg in self.messages:
            if "/users/me/messages/%s?" % msg["id"] in url:
                return dict(msg)
        raise AssertionError("unexpected request: %s %s" % (method, url))

    def gmail_mutations(self):
        return [c for c in self.calls if "/modify" in c["url"]]

    def linear_creates(self):
        return [c for c in self.calls if c["url"] == sweep.LINEAR_URL]

    def pings(self):
        return [c for c in self.calls if c["url"].startswith("https://hc.example/")]

    def first_index(self, needle):
        """Position of the first call whose url contains `needle`, or None."""
        for index, call in enumerate(self.calls):
            if needle in call["url"]:
                return index
        return None


class GmailFlowTest(unittest.TestCase):
    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def run_gmail(self, fake, dry_run=False):
        sweep.http_json = fake
        return sweep.run_adapter(sweep.GmailAdapter(CFG), dry_run)

    def test_labelled_message_creates_then_unlabels(self):
        fake = FakeHttp()
        result = self.run_gmail(fake)

        self.assertTrue(result.ok)
        self.assertEqual(result.failures, [])
        self.assertEqual(result.name, "gmail")
        self.assertEqual(result.healthcheck_url, CFG.gmail_healthcheck_url)

        creates = fake.linear_creates()
        self.assertEqual(len(creates), 1)
        fields = creates[0]["body"]["variables"]["input"]
        self.assertEqual(fields["id"], sweep.deterministic_v4("msg-1", sweep.GMAIL_NAMESPACE))
        self.assertEqual(fields["title"], "Fwd: the thing")
        self.assertIn("Thread: https://mail.google.com/mail/u/0/#all/thr-1",
                      fields["description"])

        # Create-in-Linear-first: the create precedes the ack.
        mutation_order = [c["url"] for c in fake.calls
                         if c["url"] == sweep.LINEAR_URL or "/modify" in c["url"]]
        self.assertEqual(mutation_order[0], sweep.LINEAR_URL)
        self.assertIn("/modify", mutation_order[1])

    def test_ack_removes_only_the_capture_label(self):
        # The whole ack: removeLabelIds with exactly the capture label id.
        # No addLabelIds, no archive, no mark-read, no star, no delete.
        fake = FakeHttp()
        self.run_gmail(fake)

        mutations = fake.gmail_mutations()
        self.assertEqual(len(mutations), 1)
        self.assertIn("/users/me/messages/msg-1/modify", mutations[0]["url"])
        self.assertEqual(mutations[0]["body"], {"removeLabelIds": [LABEL_ID]})

    def test_enumeration_asks_only_for_labelled_messages(self):
        fake = FakeHttp()
        self.run_gmail(fake)
        listings = [c for c in fake.calls if "/users/me/messages?" in c["url"]]
        self.assertEqual(len(listings), 1)
        self.assertIn("labelIds=%s" % LABEL_ID, listings[0]["url"])

    def test_enumeration_includes_labelled_spam_and_trash(self):
        # The label is the admission rule, not the mailbox location: Gmail
        # defaults includeSpamTrash to false, which would silently drop a
        # deliberately labelled message while still reporting success.
        fake = FakeHttp()
        self.run_gmail(fake)
        listings = [c for c in fake.calls if "/users/me/messages?" in c["url"]]
        self.assertIn("includeSpamTrash=true", listings[0]["url"])

    def test_label_removed_during_enumeration_cancels_the_capture(self):
        # The user unlabelled the message between the listing and the metadata
        # fetch. Fail closed: no issue, no mutation, and the run stays green.
        fake = FakeHttp(messages=[message(label_ids=("INBOX",))])
        result = self.run_gmail(fake)

        self.assertTrue(result.ok)
        self.assertEqual(fake.linear_creates(), [])
        self.assertEqual(fake.gmail_mutations(), [])

    def test_missing_label_fails_closed(self):
        # No capture label in the mailbox means no gesture to trust: the
        # adapter must fail visibly and enumerate or modify nothing at all.
        fake = FakeHttp(labels={"labels": [{"id": "INBOX", "name": "INBOX"}]})
        result = self.run_gmail(fake)

        self.assertFalse(result.ok)
        self.assertTrue(any("failing closed" in line for line in result.failures))
        self.assertEqual(fake.linear_creates(), [])
        self.assertEqual(fake.gmail_mutations(), [])
        self.assertFalse(any("/users/me/messages" in c["url"] for c in fake.calls))

    def test_retry_after_crash_before_ack_is_idempotent(self):
        # The crash-window replay: the issue already exists from the previous
        # run, so the create resolves to "existed" and the ack still happens.
        exists = {
            "_status": 400,
            "errors": [
                {
                    "message": "Entity Issue with id abc already exists.",
                    "extensions": {
                        "code": "INPUT_ERROR",
                        "userPresentableMessage": "Entity Issue with id abc already exists.",
                    },
                }
            ],
        }
        fake = FakeHttp(linear_responses=[exists])
        result = self.run_gmail(fake)

        self.assertTrue(result.ok)
        self.assertEqual(len(fake.linear_creates()), 1)
        self.assertEqual(len(fake.gmail_mutations()), 1)

    def test_failed_ack_leaves_the_label_for_retry_and_fails_the_run(self):
        fake = FakeHttp()
        real_call = fake.__call__

        def flaky(url, method="GET", headers=None, body=None):
            if "/modify" in url:
                fake.calls.append({"url": url, "method": method,
                                   "headers": headers, "body": body})
                return {"_status": 503, "error": "backend"}
            return real_call(url, method, headers, body)

        sweep.http_json = flaky
        result = sweep.run_adapter(sweep.GmailAdapter(CFG), False)

        self.assertFalse(result.ok)
        self.assertTrue(any("message msg-1" in line for line in result.failures))

    def test_dry_run_mutates_nothing(self):
        fake = FakeHttp()
        result = self.run_gmail(fake, dry_run=True)
        self.assertTrue(result.ok)
        self.assertEqual(fake.linear_creates(), [])
        self.assertEqual(fake.gmail_mutations(), [])


class AdapterIsolationTest(unittest.TestCase):
    """One run, both adapters; a failure in either never stops the other."""

    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def by_name(self, results):
        return {result.name: result for result in results}

    def test_run_sweep_drains_both_adapters(self):
        fake = FakeHttp()
        sweep.http_json = fake
        results = self.by_name(sweep.run_sweep(CFG, False))

        self.assertTrue(results["google-tasks"].ok)
        self.assertTrue(results["gmail"].ok)
        # Each adapter reports against its own check, never a shared one.
        self.assertEqual(results["google-tasks"].healthcheck_url, CFG.healthcheck_url)
        self.assertEqual(results["gmail"].healthcheck_url, CFG.gmail_healthcheck_url)
        self.assertEqual(len(fake.linear_creates()), 2)  # one task + one message

    def test_gmail_failure_leaves_tasks_draining_normally(self):
        fake = FakeHttp(labels={"labels": []})  # capture label missing
        sweep.http_json = fake
        results = self.by_name(sweep.run_sweep(CFG, False))

        self.assertFalse(results["gmail"].ok)
        self.assertTrue(results["google-tasks"].ok)
        patches = [c for c in fake.calls
                   if c["method"] == "PATCH" and c["url"].startswith(sweep.TASKS_URL)]
        self.assertEqual(len(patches), 1)  # the task was still swept and completed

    def test_tasks_failure_leaves_gmail_draining_normally(self):
        fake = FakeHttp(lists_response={"_status": 503, "error": "backend down"})
        sweep.http_json = fake
        results = self.by_name(sweep.run_sweep(CFG, False))

        self.assertFalse(results["google-tasks"].ok)
        self.assertTrue(results["gmail"].ok)
        self.assertEqual(len(fake.gmail_mutations()), 1)  # message swept and unlabelled

    def test_a_missing_check_url_fails_only_its_own_adapter(self):
        # A healthcheck url belongs to one adapter. A missing Gmail check must
        # not stop the Google Tasks drain -- and the Gmail adapter must not
        # drain unreported, so it captures nothing and fails visibly.
        fake = FakeHttp()
        sweep.http_json = fake
        results = self.by_name(sweep.run_sweep(CFG._replace(gmail_healthcheck_url=""), False))

        self.assertTrue(results["google-tasks"].ok)
        self.assertFalse(results["gmail"].ok)
        self.assertTrue(
            any("GMAIL_HEALTHCHECK_URL" in line for line in results["gmail"].failures)
        )
        self.assertEqual(fake.gmail_mutations(), [])
        self.assertFalse(any("/users/me/" in c["url"] for c in fake.calls))
        # The task was still swept and completed.
        patches = [c for c in fake.calls
                   if c["method"] == "PATCH" and c["url"].startswith(sweep.TASKS_URL)]
        self.assertEqual(len(patches), 1)

    def test_a_missing_check_url_is_irrelevant_to_a_dry_run(self):
        fake = FakeHttp()
        sweep.http_json = fake
        results = self.by_name(
            sweep.run_sweep(CFG._replace(healthcheck_url="", gmail_healthcheck_url=""), True)
        )
        self.assertTrue(results["google-tasks"].ok)
        self.assertTrue(results["gmail"].ok)

    def test_failure_reports_never_cross_adapters(self):
        fake = FakeHttp(labels={"labels": []})
        sweep.http_json = fake
        results = self.by_name(sweep.run_sweep(CFG, False))

        self.assertEqual(results["google-tasks"].failures, [])
        self.assertTrue(results["gmail"].failures)
        self.assertFalse(any("task" in line for line in results["gmail"].failures))


class MainReportingTest(unittest.TestCase):
    """One run, end to end through main(): which check each adapter pings,
    with what body, and when. The routing is the reporting isolation
    (ADR-0002) -- a crossed url or a deferred ping is invisible to every test
    that stops at the AdapterResult."""

    TASKS_CHECK = "https://hc.example/tasks"
    GMAIL_CHECK = "https://hc.example/gmail"
    ENV = {
        "GOOGLE_CLIENT_ID": "cid",
        "GOOGLE_CLIENT_SECRET": "secret",
        "GOOGLE_REFRESH_TOKEN": "refresh",
        "LINEAR_API_KEY": "lin_key",
        "HEALTHCHECK_URL": TASKS_CHECK,
        "GMAIL_HEALTHCHECK_URL": GMAIL_CHECK,
        "SWEEP_DENYLIST": "/nonexistent/denylist.json",
    }

    def setUp(self):
        self.real_http = sweep.http_json
        self.lock_dir = tempfile.mkdtemp()

    def tearDown(self):
        sweep.http_json = self.real_http
        shutil.rmtree(self.lock_dir, ignore_errors=True)

    def run_main(self, fake, argv=(), env=None):
        sweep.http_json = fake
        environ = dict(self.ENV)
        environ["SWEEP_LOCK"] = str(Path(self.lock_dir) / "sweep.lock")
        environ.update(env or {})
        with mock.patch.dict(os.environ, environ, clear=True):
            with contextlib.redirect_stdout(io.StringIO()):
                return sweep.main(list(argv))

    def ping(self, fake, url):
        matches = [c for c in fake.pings() if c["url"] == url]
        self.assertEqual(len(matches), 1, "expected exactly one ping to %s" % url)
        return matches[0]

    def test_each_adapter_pings_its_own_check(self):
        fake = FakeHttp()
        self.assertEqual(self.run_main(fake), 0)

        self.assertEqual(
            sorted(c["url"] for c in fake.pings()),
            [self.GMAIL_CHECK, self.TASKS_CHECK],
        )

    def test_a_failing_adapter_fails_only_its_own_check(self):
        # The capture label is gone, so Gmail fails and Google Tasks does not.
        # The fail ping must go to the Gmail check and nowhere else.
        fake = FakeHttp(labels={"labels": []})
        self.assertEqual(self.run_main(fake), 1)

        gmail = self.ping(fake, self.GMAIL_CHECK + "/fail")
        self.assertIn("failing closed", gmail["body"].decode("utf-8"))
        self.assertEqual(self.ping(fake, self.TASKS_CHECK)["method"], "GET")
        self.assertIsNone(fake.first_index(self.TASKS_CHECK + "/fail"))

    def test_a_failing_adapter_never_reports_through_the_other_check(self):
        fake = FakeHttp(lists_response={"_status": 503, "error": "backend down"})
        self.assertEqual(self.run_main(fake), 1)

        tasks = self.ping(fake, self.TASKS_CHECK + "/fail")
        self.assertIn("backend down", tasks["body"].decode("utf-8"))
        # Gmail drained fine, so its check gets a plain success and none of
        # the Google Tasks failure text.
        gmail = self.ping(fake, self.GMAIL_CHECK)
        self.assertEqual(gmail["method"], "GET")
        self.assertIsNone(gmail["body"])
        self.assertIsNone(fake.first_index(self.GMAIL_CHECK + "/fail"))

    def test_each_adapter_pings_before_the_next_one_starts(self):
        # Reporting is isolated in time too: a slow adapter must not hold an
        # earlier adapter's ping past its grace period and turn a healthy
        # drain red. The Google Tasks ping must land before Gmail's first call.
        fake = FakeHttp()
        self.run_main(fake)

        self.assertLess(
            fake.first_index(self.TASKS_CHECK),
            fake.first_index("/users/me/labels"),
        )

    def test_set_aside_counts_reach_only_their_own_check(self):
        # An empty Google Tasks row is set aside, so its note belongs in the
        # Google Tasks ping body -- and nowhere near the Gmail one.
        fake = FakeHttp()
        fake.tasks = {"items": [{"id": "task-1", "title": "", "notes": ""}]}
        self.assertEqual(self.run_main(fake), 0)

        tasks = self.ping(fake, self.TASKS_CHECK)
        self.assertEqual(tasks["method"], "POST")
        self.assertIn("1 empty captures skipped", tasks["body"].decode("utf-8"))
        self.assertIsNone(self.ping(fake, self.GMAIL_CHECK)["body"])

    def test_a_config_failure_fails_every_check(self):
        # Nothing was swept, so no adapter may look alive.
        fake = FakeHttp()
        self.assertEqual(self.run_main(fake, env={"LINEAR_API_KEY": ""}), 1)
        for url in (self.TASKS_CHECK, self.GMAIL_CHECK):
            body = self.ping(fake, url + "/fail")["body"].decode("utf-8")
            self.assertIn("LINEAR_API_KEY", body)

    def test_a_dry_run_pings_nothing(self):
        fake = FakeHttp()
        self.assertEqual(self.run_main(fake, argv=["--dry-run"]), 0)
        self.assertEqual(fake.pings(), [])


class GmailConfigTest(unittest.TestCase):
    ENV = {
        "GOOGLE_CLIENT_ID": "cid",
        "GOOGLE_CLIENT_SECRET": "secret",
        "GOOGLE_REFRESH_TOKEN": "refresh",
        "LINEAR_API_KEY": "lin",
        "HEALTHCHECK_URL": "https://hc.example/tasks",
    }

    def test_a_missing_check_url_does_not_abort_config(self):
        # Config is shared by both adapters, so it must not fail on one
        # adapter's check: that check is validated at its own boundary, where
        # it can fail only its own drain.
        cfg = sweep.config_from_env(dict(self.ENV))
        self.assertEqual(cfg.gmail_healthcheck_url, "")
        self.assertEqual(cfg.healthcheck_url, "https://hc.example/tasks")

    def test_a_missing_credential_still_aborts_config(self):
        env = dict(self.ENV)
        del env["LINEAR_API_KEY"]
        with self.assertRaises(sweep.SweepError) as ctx:
            sweep.config_from_env(env)
        self.assertIn("LINEAR_API_KEY", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
