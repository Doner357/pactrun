"""CI scope tests, including real Git diffs; temporary files stay in target/."""

import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

from ci_scope import changed_paths, requires_runtime

ROOT = Path(__file__).resolve().parents[1]


class ScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        (ROOT / "target").mkdir(exist_ok=True)

    def test_docs_only(self):
        self.assertFalse(requires_runtime(["README.md", "docs/guides/installation.md", "website/pnpm-lock.yaml"]))

    def test_runtime_unknown_spec_and_empty_fail_closed(self):
        for path in ["src/main.rs", "Cargo.lock", "tests/example.md", "docs/spec/schema.md",
                     "docs/schema.sql", ".github/workflows/ci.yml", "tools/ci_scope.py", "new-area/file"]:
            with self.subTest(path=path):
                self.assertTrue(requires_runtime(["README.md", path]))
        self.assertTrue(requires_runtime([]))

    def test_unknown_initial_and_invalid_events(self):
        cases = [("workflow_dispatch", {}), ("push", {"before": "0" * 40}),
                 ("push", {"before": "--output=elsewhere"})]
        for name, event in cases:
            with self.assertRaises(ValueError):
                changed_paths(name, event, "a" * 40, ROOT)

    def test_real_diff_includes_deletions_and_more_than_300_files(self):
        target = ROOT / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="ci-scope-", dir=target) as directory:
            repo = Path(directory)
            env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                       GIT_AUTHOR_NAME="CI fixture", GIT_AUTHOR_EMAIL="ci@example.invalid",
                       GIT_COMMITTER_NAME="CI fixture", GIT_COMMITTER_EMAIL="ci@example.invalid")

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=repo, env=env, stderr=subprocess.PIPE).decode().strip()

            git("init")
            (repo / "source.rs").write_text("runtime", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "base")
            base = git("rev-parse", "HEAD")
            (repo / "docs").mkdir()
            (repo / "source.rs").rename(repo / "docs/moved.md")
            for index in range(310):
                (repo / f"docs/{index}.md").write_text("documentation", encoding="utf-8")
            git("add", "--all")
            git("commit", "-m", "change")
            head = git("rev-parse", "HEAD")
            for event_name, event in [("push", {"before": base}),
                                      ("pull_request", {"pull_request": {"base": {"sha": base}}})]:
                paths = changed_paths(event_name, event, head, repo)
                self.assertEqual(len(paths), 312)
                self.assertIn("source.rs", paths)
                self.assertTrue(requires_runtime(paths))

    def test_missing_history_selects_full_verification(self):
        with tempfile.TemporaryDirectory(prefix="ci-event-", dir=ROOT / "target") as directory:
            root = Path(directory)
            event = root / "event.json"
            event.write_text(json.dumps({"before": "a" * 40}), encoding="utf-8")
            output = root / "output"
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_EVENT_NAME="push",
                       GITHUB_SHA="b" * 40, GITHUB_OUTPUT=str(output))
            subprocess.run([sys.executable, str(ROOT / "tools/ci_scope.py")], cwd=ROOT, env=env, check=True,
                           stdout=subprocess.PIPE)
            self.assertEqual(output.read_text(encoding="utf-8"), "runtime=true\n")

    @unittest.skipIf(os.name == "nt", "CI gate uses the Ubuntu bash runner; exercised on Linux")
    def test_actual_aggregate_gate(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        script = textwrap.dedent(workflow.split("# BEGIN CI GATE\n", 1)[1].split("# END CI GATE", 1)[0])
        cases = [
            ("success", "true", "success", "success", True),
            ("success", "false", "success", "skipped", True),
            ("failure", "false", "success", "skipped", False),
            ("success", "true", "success", "skipped", False),
            ("success", "false", "failure", "skipped", False),
            ("success", "true", "success", "cancelled", False),
            ("success", "", "success", "skipped", False),
            ("skipped", "false", "skipped", "skipped", False),
            ("success", "false", "success", "failure", False),
        ]
        for scope, runtime, ubuntu, windows, expected in cases:
            for debian in ['success', 'skipped', 'failure', 'cancelled', '']:
                allowed = expected and debian == ('success' if runtime == 'true' else 'skipped')
                with self.subTest(scope=scope, runtime=runtime, ubuntu=ubuntu, windows=windows, debian=debian):
                    env = dict(os.environ, SCOPE_RESULT=scope, RUNTIME=runtime,
                               UBUNTU_RESULT=ubuntu, WINDOWS_RESULT=windows, DEBIAN12_RESULT=debian)
                    result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], env=env,
                                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    self.assertEqual(result.returncode == 0, allowed, result.stderr.decode())


if __name__ == "__main__":
    unittest.main()
