"""Cred-free, toolchain-free tests for the local review candidates
(`scripts/candidate/`; `candidate.sh`'s header is the component doc).

A real candidate needs cargo, wasm-pack, pnpm and minutes of building, so
end to end is checked by hand. What is decidable without any of that is
tested here, against a fake slot tree in a temp dir:

- the production guard (`guard.sh`) passes a clean slot and refuses each
  route to production it exists to catch -- the plan's "guard refuses a
  production URL" is the preview-target cases;
- the configs `candidate.sh` generates pass that guard, and the wrangler
  generator refuses a revision whose config it has not been taught;
- the startup binding-table check fails closed;
- `pick` never chooses the newest good candidate by default;
- neither script names the production host, the operator mint scripts,
  the real token's path, or a deploying/remote wrangler command.
"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAND = ROOT / "scripts/candidate"
GUARD = CAND / "guard.sh"
CANDIDATE = CAND / "candidate.sh"

FORBIDDEN_ENV = [
    "VITE_API_BASE_URL", "HB_API_BASE", "HB_API_TOKEN", "HB_API_TOKEN_PATH",
    "CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_API_KEY",
    "CLOUDFLARE_EMAIL", "CLOUDFLARE_ENV", "CLOUDFLARE_API_BASE_URL",
    "CF_API_TOKEN", "CF_ACCOUNT_ID", "HB_CANDIDATE_ROOT",
]

GOOD_TABLE = """\
Using secrets defined in admin.env
Your Worker has access to the following bindings:
Binding                            Resource                  Mode
env.AUTHORITY (Authority)          Durable Object            local
env.ADMIN_SECRET ("(hidden)")      Environment Variable      local

Ready on http://127.0.0.1:8801
"""


class SlotCase(unittest.TestCase):
    """A fake root with a clean slot `a` (and an empty HOME)."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="cand-test-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.home = self.tmp / "home"
        self.home.mkdir()
        self.root = self.tmp / "root"
        self.slot = self.make_slot(self.root, "a")

    def env(self, **extra):
        env = {k: v for k, v in os.environ.items() if k not in FORBIDDEN_ENV}
        env["HOME"] = str(self.home)
        env.update(extra)
        return env

    def make_slot(self, root, slot):
        worker, web = {"a": (8801, 4801), "b": (8802, 4802)}[slot]
        sd = root / slot
        wt = sd / f"hb-{slot}"
        state = sd / "state"
        (wt / "client/web/dist/assets").mkdir(parents=True)
        (wt / "client/web/dist/index.html").write_text("<!doctype html><title>hb</title>")
        (wt / "client/web/dist/assets/app.js").write_text("fetch(self.location.origin + '/api/changes')")
        (wt / "client/web/.env.example").write_text("# VITE_API_BASE_URL=\n")
        (wt / "server/worker/build/worker").mkdir(parents=True)
        (wt / "server/worker/build/worker/shim.mjs").write_text("export default {}")
        (wt / "server/worker/build/index_bg.wasm").write_bytes(b"\x00asm\x01\x00\x00\x00")
        state.mkdir(parents=True)
        self.write_wrangler(sd, {})
        self.write_preview(sd, f"http://127.0.0.1:{worker}", port=web)
        admin = state / "admin.env"
        admin.write_text("ADMIN_SECRET=" + "ab" * 32 + "\n")
        admin.chmod(0o600)
        return sd

    def write_wrangler(self, sd, overrides):
        slot = sd.name
        c = {
            "name": "hummingbird-authority",
            "main": str(sd / f"hb-{slot}/server/worker/build/worker/shim.mjs"),
            "compatibility_date": "2026-08-08",
            "durable_objects": {"bindings": [{"name": "AUTHORITY", "class_name": "Authority"}]},
            "migrations": [{"tag": "v1", "new_sqlite_classes": ["Authority"]}],
        }
        c.update(overrides)
        (sd / "state/wrangler.json").write_text(json.dumps(c, indent=2))

    def write_preview(self, sd, target, port=None, **preview):
        slot = sd.name
        p = {"host": "127.0.0.1", "port": port or {"a": 4801, "b": 4802}[slot],
             "strictPort": True, "proxy": {"/api": {"target": target, "changeOrigin": False}}}
        p.update(preview)
        c = {"root": str(sd / f"hb-{slot}/client/web"), "preview": p}
        (sd / "state/vite.preview.config.mjs").write_text("export default " + json.dumps(c) + ";\n")

    def guard(self, slot_dir=None, **env):
        return subprocess.run(
            ["bash", str(GUARD), str(slot_dir or self.slot)],
            env=self.env(**env), capture_output=True, text=True,
        )

    def assertRefused(self, result, fragment):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        refusals = [l for l in result.stderr.splitlines() if l.startswith("REFUSED: ")]
        self.assertTrue(any(fragment in l for l in refusals),
                        f"no REFUSED line containing {fragment!r} in:\n{result.stderr}")

    def candidate(self, script, **env):
        """Run bash with candidate.sh sourced (functions only), then `script`."""
        return subprocess.run(
            ["bash", "-c", f'. "{CANDIDATE}"; {script}'],
            env=self.env(HB_CANDIDATE_ROOT=str(self.root), **env),
            capture_output=True, text=True,
        )


class GuardTest(SlotCase):
    def test_clean_slot_passes(self):
        r = self.guard()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("REFUSED", r.stderr)

    def test_production_env_is_refused(self):
        for name, value in [
            ("VITE_API_BASE_URL", "https://hb.twinion.net"),
            ("CLOUDFLARE_API_TOKEN", "x"),
            ("HB_API_BASE", "https://hb.twinion.net"),
            ("CLOUDFLARE_ACCOUNT_ID", "x"),
        ]:
            with self.subTest(name=name):
                self.assertRefused(self.guard(**{name: value}), name)

    def test_legacy_wrangler_login_dir_is_refused(self):
        (self.home / ".wrangler").mkdir()
        self.assertRefused(self.guard(), ".wrangler")

    def test_preview_proxy_to_anything_but_this_slots_worker_is_refused(self):
        for target in [
            "https://hb.twinion.net",
            "https://foo.workers.dev",
            "http://127.0.0.1:8802",  # slot b's worker, from slot a
            "http://localhost:8801",
        ]:
            with self.subTest(target=target):
                self.write_preview(self.slot, target)
                self.assertRefused(self.guard(), "preview")

    def test_a_production_url_anywhere_in_the_preview_config_is_refused(self):
        self.write_preview(self.slot, "http://127.0.0.1:8801",
                           headers={"x": "https://hb.twinion.net"})
        self.assertRefused(self.guard(), "https://hb.twinion.net")

    def test_preview_must_bind_loopback_strictly(self):
        self.write_preview(self.slot, "http://127.0.0.1:8801", host="0.0.0.0")
        self.assertRefused(self.guard(), "preview host")
        self.write_preview(self.slot, "http://127.0.0.1:8801", strictPort=False)
        self.assertRefused(self.guard(), "strictPort")

    def test_wrangler_config_outside_the_allowlist_is_refused(self):
        cases = [
            ({"routes": [{"pattern": "hb.twinion.net/api/*", "zone_name": "twinion.net"}]}, "routes"),
            ({"workers_dev": True}, "workers_dev"),
            ({"secrets": {"required": ["ADMIN_SECRET"]}}, "secrets"),
            ({"vars": {"RUNNER_BASE_URL": "https://x.fly.dev"}}, "vars"),
            ({"name": "hummingbird-authority-staging"}, "name"),
            ({"durable_objects": {"bindings": [
                {"name": "AUTHORITY", "class_name": "Authority", "remote": True}]}}, "remote"),
            ({"migrations": [{"tag": "v1", "remote": True}]}, "remote"),
            ({"main": "/elsewhere/shim.mjs"}, "main"),
        ]
        for overrides, fragment in cases:
            with self.subTest(fragment=fragment):
                self.write_wrangler(self.slot, overrides)
                self.assertRefused(self.guard(), fragment)

    def test_production_hostname_in_built_artifacts_is_refused(self):
        js = self.slot / "hb-a/client/web/dist/assets/x.js"
        js.write_text('const base = "https://hb.twinion.net";')
        self.assertRefused(self.guard(), "x.js")
        js.unlink()
        wasm = self.slot / "hb-a/server/worker/build/worker/extra.wasm"
        wasm.write_bytes(b"\x00asm\x00x.workers.dev\x00")
        self.assertRefused(self.guard(), "extra.wasm")

    def test_missing_artifacts_fail_closed(self):
        shutil.rmtree(self.slot / "hb-a/client/web/dist")
        self.assertRefused(self.guard(), "dist")

    def test_env_and_dev_vars_files_are_refused(self):
        cases = [
            ("hb-a/server/worker/.dev.vars", ".dev.vars"),
            ("hb-a/server/worker/.env", ".env"),
            ("hb-a/client/web/.env.local", ".env.local"),
            ("hb-a/client/web/.env.production", ".env.production"),
            ("state/.dev.vars", ".dev.vars"),
        ]
        for rel, fragment in cases:
            with self.subTest(rel=rel):
                f = self.slot / rel
                f.write_text("X=1\n")
                self.assertRefused(self.guard(), fragment)
                f.unlink()

    def test_admin_env_must_be_private_and_single_keyed(self):
        admin = self.slot / "state/admin.env"
        admin.chmod(0o644)
        self.assertRefused(self.guard(), "mode 644")
        admin.chmod(0o600)
        admin.write_text("ADMIN_SECRET=" + "ab" * 32 + "\nRUNNER_BEARER_TOKEN=x\n")
        self.assertRefused(self.guard(), "exactly one ADMIN_SECRET")

    def test_root_inside_a_git_work_tree_is_refused(self):
        repo = self.tmp / "repo"
        repo.mkdir()
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        slot = self.make_slot(repo / "root", "a")
        self.assertRefused(self.guard(slot), "inside a git work tree")


class GeneratedConfigTest(SlotCase):
    def test_generated_configs_pass_the_guard(self):
        shutil.copy(ROOT / "server/worker/wrangler.toml", self.slot / "hb-a/server/worker/wrangler.toml")
        r = self.candidate("init_root; slot_paths a; gen_wrangler_config && gen_preview_config")
        self.assertEqual(r.returncode, 0, r.stderr)
        c = json.loads((self.slot / "state/wrangler.json").read_text())
        self.assertNotIn("routes", c)
        self.assertNotIn("build", c)
        self.assertEqual(c["name"], "hummingbird-authority")
        g = self.guard()
        self.assertEqual(g.returncode, 0, g.stderr)

    def test_wrangler_generator_refuses_keys_it_was_not_taught(self):
        toml = (ROOT / "server/worker/wrangler.toml").read_text()
        toml += '\n[[kv_namespaces]]\nbinding = "CACHE"\nid = "abc"\n'
        (self.slot / "hb-a/server/worker/wrangler.toml").write_text(toml)
        r = self.candidate("init_root; slot_paths a; gen_wrangler_config")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("kv_namespaces", r.stderr)


class BindingCheckTest(SlotCase):
    def check(self, log):
        (self.slot / "logs").mkdir(exist_ok=True)
        (self.slot / "logs/wrangler.log").write_text(log)
        return self.candidate("init_root; slot_paths a; check_bindings")

    def test_local_authority_and_admin_secret_pass(self):
        r = self.check(GOOD_TABLE)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_a_remote_row_fails(self):
        r = self.check(GOOD_TABLE.replace("Durable Object            local",
                                          "Durable Object            remote"))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not local", r.stderr)

    def test_an_extra_binding_fails(self):
        r = self.check(GOOD_TABLE.replace(
            "env.ADMIN_SECRET",
            'env.RUNNER_BEARER_TOKEN ("(hidden)")      Environment Variable      local\nenv.ADMIN_SECRET'))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("RUNNER_BEARER_TOKEN", r.stderr)

    def test_a_missing_admin_secret_fails(self):
        r = self.check(GOOD_TABLE.replace(
            'env.ADMIN_SECRET ("(hidden)")      Environment Variable      local\n', ""))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("ADMIN_SECRET", r.stderr)

    def test_no_table_fails(self):
        r = self.check("Ready on http://127.0.0.1:8801\n")
        self.assertNotEqual(r.returncode, 0)


class PickTest(SlotCase):
    SHA1, SHA2, SHA3 = "1" * 40, "2" * 40, "3" * 40

    def manifest(self, slot, sha, status, built_at):
        d = self.root / slot
        d.mkdir(parents=True, exist_ok=True)
        (d / "manifest.json").write_text(json.dumps(
            {"slot": slot, "sha": sha, "status": status, "built_at": built_at}))

    def pick(self, sha):
        r = subprocess.run(["bash", str(CANDIDATE), "pick", sha],
                           env=self.env(HB_CANDIDATE_ROOT=str(self.root)),
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.strip()

    def test_empty_root_picks_a(self):
        shutil.rmtree(self.root)
        self.assertEqual(self.pick(self.SHA1), "a")

    def test_a_failed_slot_is_reused_before_a_good_one(self):
        self.manifest("a", self.SHA1, "ok", "2026-10-02T10:00:00Z")
        self.manifest("b", self.SHA2, "failed", None)
        self.assertEqual(self.pick(self.SHA3), "b")

    def test_both_good_replaces_the_older(self):
        self.manifest("a", self.SHA1, "ok", "2026-10-02T11:00:00Z")
        self.manifest("b", self.SHA2, "ok", "2026-10-02T10:00:00Z")
        self.assertEqual(self.pick(self.SHA3), "b")
        self.manifest("b", self.SHA2, "ok", "2026-10-02T12:00:00Z")
        self.assertEqual(self.pick(self.SHA3), "a")

    def test_the_same_sha_keeps_its_own_slot(self):
        self.manifest("a", self.SHA1, "ok", "2026-10-02T09:00:00Z")
        self.manifest("b", self.SHA2, "ok", "2026-10-02T10:00:00Z")
        self.assertEqual(self.pick(self.SHA2), "b")
        self.assertEqual(self.pick(self.SHA1), "a")


class ForbiddenStringsTest(unittest.TestCase):
    NEVER = ["mint-device-token", "mint-hb-token", "hb.twinion.net",
             "wrangler deploy", "wrangler publish", "wrangler login",
             "secret put", "--remote"]

    def test_candidate_sh_names_no_production_path(self):
        text = CANDIDATE.read_text()
        for s in self.NEVER + [".config/hummingbird", "api-token"]:
            with self.subTest(s=s):
                self.assertNotIn(s, text)

    def test_guard_sh_names_none_but_what_it_refuses(self):
        text = GUARD.read_text()
        for s in self.NEVER + ["api-token"]:
            with self.subTest(s=s):
                self.assertNotIn(s, text)


if __name__ == "__main__":
    unittest.main()
