"""CI scope tests, including real Git diffs; temporary files stay in target/."""

import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

from ci_scope import changed_paths, delivery_metadata, requires_browser, requires_runtime

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

    def test_generated_delivery_metadata_does_not_require_product_retests(self):
        paths = ['releases/catalog.json', 'releases/1.0.0-alpha.9.json',
                 'bucket/pactrun-preview.json', 'Formula/pactrun-preview.rb']
        self.assertFalse(requires_runtime(paths))
        self.assertTrue(all(delivery_metadata(path) for path in paths))
        for path in ['tools/release_artifacts.py', 'xtask/src/release_sources.rs',
                     '.github/workflows/release-candidate.yml', 'releases/schema.sql',
                     'src/main.rs', 'Cargo.toml', '.config/nextest.toml']:
            with self.subTest(path=path):
                self.assertTrue(requires_runtime([*paths, path]))

    def test_browser_scope_includes_rendering_inputs_but_not_plain_runtime_code(self):
        self.assertTrue(requires_browser([]))
        for path in ['website/pnpm-lock.yaml','docs/spec/example.md','src/cli/help_catalog.json',
                     'tools/browser/pnpm-lock.yaml','.github/workflows/ci.yml']:
            self.assertTrue(requires_browser([path]))
        self.assertFalse(requires_browser(['src/domain/migration.rs','releases/catalog.json']))

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
            self.assertEqual(output.read_text(encoding="utf-8"), "runtime=true\ndelivery=true\nbrowser=true\n")

    @unittest.skipIf(os.name == "nt", "CI gate uses the Ubuntu bash runner; exercised on Linux")
    def test_actual_aggregate_gate(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        script = textwrap.dedent(workflow.split("# BEGIN CI GATE\n", 1)[1].split("# END CI GATE", 1)[0])
        cases = [
            ("success", "true", "true", "success", "success", True),
            ("success", "false", "false", "success", "skipped", True),
            ("success", "false", "true", "success", "success", True),
            ("success", "false", "true", "success", "skipped", False),
            ("success", "true", "false", "success", "success", False),
            ("failure", "false", "false", "success", "skipped", False),
            ("success", "true", "true", "success", "skipped", False),
            ("success", "false", "false", "failure", "skipped", False),
            ("success", "true", "true", "success", "cancelled", False),
            ("success", "", "false", "success", "skipped", False),
            ("skipped", "false", "false", "skipped", "skipped", False),
            ("success", "false", "false", "success", "failure", False),
        ]
        for scope, runtime, delivery, ubuntu, windows, expected in cases:
            for debian in ['success', 'skipped', 'failure', 'cancelled', '']:
                allowed = expected and debian == ('success' if runtime == 'true' else 'skipped')
                with self.subTest(scope=scope, runtime=runtime, ubuntu=ubuntu, windows=windows, debian=debian):
                    env = dict(os.environ, SCOPE_RESULT=scope, RUNTIME=runtime, DELIVERY=delivery,
                               UBUNTU_RESULT=ubuntu, WINDOWS_RESULT=windows, DEBIAN12_RESULT=debian)
                    result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], env=env,
                                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    self.assertEqual(result.returncode == 0, allowed, result.stderr.decode())

    def test_metadata_consumers_and_docs_still_run_when_runtime_is_reused(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text(encoding='utf-8')
        self.assertEqual(workflow.count('Verify current delivery consumers when runtime was omitted or reused'), 2)
        self.assertEqual(workflow.count('-B -m unittest tools.test_native_package_acceptance'), 2)
        self.assertIn('Verify documentation and build the current site', workflow)
        self.assertIn('if-no-files-found: ignore', workflow)


if __name__ == "__main__":
    unittest.main()
