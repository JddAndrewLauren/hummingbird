"""Drift gate for the root `Dockerfile`'s poller build (#774, #789).

The builder stage has one `RUN cargo build --release ...` line naming every
poller package with `-p` and every binary with `--bin`. Cargo applies
`--bin` as a target filter across EVERY `-p` in the command, so a package
whose default bin is not itself named with `--bin` is silently not built:
the build finishes green and the `COPY --from=poller-builder` lines below
are the first thing to notice. That is exactly what shipped in #774 (only
the graph/github-status bins were named, so `hummingbird-gmail-poll` and
`hummingbird-calendar-poll` were dropped), was found only by a local
`docker build`, and broke the first deploy on main (fixed in #789).

CI cannot build the image on a PR -- `deploy.yml`'s `pull_request` run
reaches only its `test` job, which is `python3 -m unittest discover -s
tests` plus shellcheck; the `docker build` happens inside `flyctl deploy`,
push-to-main only. So a text-level gate is the only thing that can catch
the trap before main, and it lives HERE rather than beside the crontab
gates in `server/*/tests/contract.rs` because those run under the Rust
workflows, not under the job a `Dockerfile` edit triggers. This file is in
the one suite that job executes.

Both directions are asserted so the two lists cannot drift apart: every
`COPY --from=poller-builder .../release/<name>` has a `--bin <name>`, and
every `--bin <name>` is copied out of the builder.
"""

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCKERFILE = ROOT / "Dockerfile"

COPY_FROM_BUILDER = re.compile(
    r"^COPY\s+--from=poller-builder\s+/src/server/target/release/(\S+)\s+\S+\s*$",
    re.MULTILINE,
)


def logical_lines(text):
    """Dockerfile instructions with backslash line continuations joined."""
    return re.sub(r"\\\n", " ", text).splitlines()


def cargo_build_bins(text):
    """Every `--bin <name>` in the one `RUN cargo build` instruction."""
    runs = [
        line for line in logical_lines(text)
        if line.startswith("RUN") and "cargo build" in line
    ]
    if len(runs) != 1:
        raise AssertionError(
            f"expected exactly one `RUN cargo build` instruction, found {len(runs)}"
        )
    return re.findall(r"--bin\s+(\S+)", runs[0])


def copied_bins(text):
    """Every `<name>` copied out of `/src/server/target/release/`."""
    return COPY_FROM_BUILDER.findall(text)


def assert_bins_agree(text):
    built = cargo_build_bins(text)
    copied = copied_bins(text)
    if not copied:
        raise AssertionError("no `COPY --from=poller-builder` lines found")
    missing = [name for name in copied if name not in built]
    if missing:
        raise AssertionError(
            "copied out of the builder but not named with `--bin` in `cargo build` "
            f"(cargo will silently not build it): {missing}"
        )
    unused = [name for name in built if name not in copied]
    if unused:
        raise AssertionError(
            "named with `--bin` in `cargo build` but never copied out of the builder: "
            f"{unused}"
        )


class DockerfilePollerBinsTest(unittest.TestCase):
    def test_every_copied_bin_is_built_and_every_built_bin_is_copied(self):
        assert_bins_agree(DOCKERFILE.read_text())

    def test_the_gate_reads_the_real_lists(self):
        # Guard against the regexes silently matching nothing: the five
        # #774 pollers must be seen on both sides.
        text = DOCKERFILE.read_text()
        expected = {
            "hummingbird-gmail-poll",
            "hummingbird-calendar-poll",
            "graph-mail-poll",
            "graph-calendar-poll",
            "github-status-poll",
        }
        self.assertEqual(set(copied_bins(text)), expected)
        self.assertEqual(set(cargo_build_bins(text)), expected)

    def test_a_missing_bin_fails(self):
        # The #774 shape: the package is still named with `-p` but its
        # default bin is no longer named with `--bin`.
        text = DOCKERFILE.read_text()
        broken = text.replace(
            "-p hummingbird-gmail-poll --bin hummingbird-gmail-poll",
            "-p hummingbird-gmail-poll",
        )
        self.assertNotEqual(text, broken)
        with self.assertRaisesRegex(AssertionError, "hummingbird-gmail-poll"):
            assert_bins_agree(broken)

    def test_an_uncopied_bin_fails(self):
        text = DOCKERFILE.read_text()
        broken = "\n".join(
            line for line in text.splitlines()
            if "/release/graph-mail-poll " not in line
        )
        self.assertNotEqual(text, broken)
        with self.assertRaisesRegex(AssertionError, "graph-mail-poll"):
            assert_bins_agree(broken)


if __name__ == "__main__":
    unittest.main()
