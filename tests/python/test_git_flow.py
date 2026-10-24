"""Release policy tests: real Git topology, disposable repo, mocked GitHub API."""
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("git_flow", ROOT / "scripts/git-flow.py")
flow = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(flow)


def pr(head="release/1.2.3", base="master", commit="abc"):
    return {"merged_at": "2026-10-25", "merge_commit_sha": commit,
            "base": {"ref": base},
            "head": {"ref": head, "repo": {"full_name": flow.REPOSITORY}}}


def run(commit="abc", run_id=10, state="success", branch="master", event="push"):
    return {"id": run_id, "head_sha": commit, "head_branch": branch, "event": event,
            "status": "completed" if state != "pending" else "in_progress",
            "conclusion": state if state != "pending" else None,
            "html_url": "https://github.com/szwathub/stool/actions/runs/10"}


class TopologyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="stool-git-flow-")
        self.previous = os.getcwd()
        os.chdir(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        self.addCleanup(os.chdir, self.previous)
        self.command("init", "-q", "-b", "master")
        self.command("config", "user.name", "Release Test")
        self.command("config", "user.email", "test@example.invalid")
        self.command("config", "commit.gpgsign", "false")
        self.command("config", "tag.gpgSign", "false")
        Path("Cargo.toml").write_text('[package]\nname="stool"\nversion="1.2.3"\n')
        Path("Cargo.lock").write_text('[[package]]\nname="stool"\nversion="1.2.3"\n')
        Path("RELEASE_NOTES.md").write_text('# stool v1.2.3\n\nRelease notes.\n')
        self.command("add", ".")
        self.command("commit", "-qm", "Initial master")
        self.root = flow.git("rev-parse", "HEAD")
        self.command("checkout", "-qb", "develop")
        Path("feature").write_text("Feature from develop")
        self.command("add", ".")
        self.command("commit", "-qm", "Develop feature")
        self.develop = flow.git("rev-parse", "HEAD")
        self.command("checkout", "-qb", "release/1.2.3")
        Path("ready").write_text("Release ready")
        self.command("add", ".")
        self.command("commit", "-qm", "Prepare release")
        self.release = flow.git("rev-parse", "HEAD")
        self.command("checkout", "-q", "master")
        self.command("merge", "--no-ff", "--no-gpg-sign", "-qm", "Merge release", "release/1.2.3")
        self.merge = flow.git("rev-parse", "HEAD")
        self.command("update-ref", "refs/remotes/origin/master", self.merge)

    def command(self, *args):
        subprocess.run(["git", *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def test_formal_release_merge_is_eligible(self):
        with patch.object(flow, "paginated", return_value=[pr(commit=self.merge)]):
            self.assertEqual(flow.formal_commit(self.merge), "1.2.3")

    def test_merged_develop_and_release_side_commits_are_rejected(self):
        for commit in (self.develop, self.release):
            self.command("merge-base", "--is-ancestor", commit, "origin/master")
            with self.assertRaisesRegex(ValueError, "first-parent"):
                flow.formal_commit(commit)

    def test_unmerged_develop_commit_is_rejected(self):
        self.command("checkout", "-q", "develop")
        self.command("commit", "--allow-empty", "-qm", "Unreleased feature")
        with self.assertRaisesRegex(ValueError, "first-parent"):
            flow.formal_commit(flow.git("rev-parse", "HEAD"))

    def test_non_merge_master_commit_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "two-parent"):
            flow.formal_commit(self.root)

    def test_merge_requires_a_merged_release_pr(self):
        for items in ([], [pr(base="develop", commit=self.merge)],
                      [pr(head="feature/no-release", commit=self.merge)],
                      [pr(head="hotfix/1.2.4", commit=self.merge)]):
            with self.subTest(items=items), patch.object(flow, "paginated", return_value=items):
                with self.assertRaises(ValueError):
                    flow.formal_commit(self.merge)

    def test_cargo_lock_and_notes_must_match(self):
        for name, data, error in (("Cargo.lock", '[[package]]\nname="stool"\nversion="1.2.2"', "Cargo.lock"),
                                  ("RELEASE_NOTES.md", "# stool v1.2.2\n", "release-notes")):
            with self.subTest(name=name):
                Path(name).write_text(data)
                self.command("add", name)
                self.command("commit", "-qm", "Broken metadata")
                with self.assertRaisesRegex(ValueError, error):
                    flow.metadata("HEAD")
                self.command("reset", "--hard", self.merge)

    def test_local_tag_entry_refuses_develop_and_dirty_master(self):
        self.command("checkout", "-q", "develop")
        with self.assertRaisesRegex(ValueError, "Switch to master"):
            flow.create_tag("v1.2.3", False)
        self.command("checkout", "-q", "master")
        Path("untracked").touch()
        with self.assertRaisesRegex(ValueError, "clean"):
            flow.create_tag("v1.2.3", False)
        self.assertEqual(flow.git("tag", "--list"), "")


class PolicyTests(unittest.TestCase):
    def test_release_and_hotfix_versions_with_optional_v(self):
        for head in ("release/1.2.3", "release/v1.2.3", "hotfix/1.2.3", "hotfix/v1.2.3"):
            flow.check_pull_request(pr(head=head), "1.2.3")
        for head in ("develop", "feature/release", "release/1.2.4", "hotfix/../master"):
            with self.assertRaises(ValueError):
                flow.check_pull_request(pr(head=head), "1.2.3")

    def test_fork_cannot_supply_release_pr(self):
        item = pr()
        item["head"]["repo"]["full_name"] = "someone/stool"
        with self.assertRaisesRegex(ValueError, "this repository"):
            flow.check_pull_request(item, "1.2.3")

    def test_develop_back_merge_and_feature_pr(self):
        for head in ("master", "feature/template", "release/1.2.3", "hotfix/1.2.4"):
            flow.check_event("pull_request", {"pull_request": pr(head=head, base="develop")})
        with self.assertRaises(ValueError):
            flow.check_event("pull_request", {"pull_request": pr(head="random", base="develop")})

    def test_manual_dispatch_must_use_master(self):
        flow.dispatch_ref("workflow_dispatch", "refs/heads/master")
        for ref in ("refs/heads/develop", "refs/tags/v1.2.3", ""):
            with self.assertRaisesRegex(ValueError, "from master"):
                flow.dispatch_ref("workflow_dispatch", ref)

    def test_check_must_be_successful_master_push_for_exact_commit(self):
        for candidate in (run(branch="develop"), run(event="pull_request"), run(commit="other"),
                          run(state="failure"), run(state="pending")):
            with self.subTest(candidate=candidate), patch.object(flow, "api", return_value={"workflow_runs": [candidate]}):
                with self.assertRaises(ValueError):
                    flow.master_check("abc")
        with patch.object(flow, "api", return_value={"workflow_runs": [run()]}):
            self.assertEqual(flow.master_check("abc")["id"], 10)

    def test_latest_failed_or_running_check_blocks_older_success(self):
        for state in ("failure", "pending"):
            with patch.object(flow, "api", return_value={"workflow_runs": [run(), run(run_id=11, state=state)]}):
                with self.assertRaises(ValueError):
                    flow.master_check("abc")

    def test_tag_requires_signed_annotated_tag_and_matching_version(self):
        ref = {"object": {"type": "tag", "sha": "object"}}
        tag = {"tag": "v1.2.3", "object": {"type": "commit", "sha": "abc"},
               "verification": {"verified": True}}
        with patch.object(flow, "api", side_effect=[ref, tag]), patch.object(flow, "release_commit", return_value=("1.2.3", run())):
            self.assertEqual(flow.verify_tag("v1.2.3")["commit"], "abc")
        with patch.object(flow, "api", return_value={"object": {"type": "commit"}}):
            with self.assertRaisesRegex(ValueError, "annotated"):
                flow.verify_tag("v1.2.3")
        for bad in ({**tag, "verification": {"verified": False}}, {**tag, "tag": "v1.2.4"}):
            with patch.object(flow, "api", side_effect=[ref, bad]):
                with self.assertRaises(ValueError):
                    flow.verify_tag("v1.2.3")
        with patch.object(flow, "api", side_effect=[ref, tag]), patch.object(flow, "release_commit", return_value=("1.2.4", run())):
            with self.assertRaisesRegex(ValueError, "version"):
                flow.verify_tag("v1.2.3")

    def test_changed_tag_object_or_commit_blocks_publication(self):
        ref = {"object": {"type": "tag", "sha": "new-object"}}
        tag = {"tag": "v1.2.3", "object": {"type": "commit", "sha": "abc"},
               "verification": {"verified": True}}
        for expected in (("wrong", None), ("abc", "old-object")):
            with patch.object(flow, "api", side_effect=[ref, tag]):
                with self.assertRaisesRegex(ValueError, "changed during release"):
                    flow.verify_tag("v1.2.3", *expected)

    def test_gate_never_grants_develop_or_pr_check_status(self):
        for branch, event_name in (("develop", "push"), ("master", "pull_request")):
            event = {"workflow_run": {**run(branch=branch, event=event_name),
                                      "head_repository": {"full_name": flow.REPOSITORY}}}
            with patch.object(flow, "api") as mock_api:
                with self.assertRaises(ValueError):
                    flow.master_gate("workflow_run", event, "refs/heads/develop")
                mock_api.assert_not_called()

    def test_gate_only_grants_status_after_provenance_and_check(self):
        event = {"workflow_run": {**run(), "head_repository": {"full_name": flow.REPOSITORY}}}
        with patch.object(flow, "formal_commit", return_value="1.2.3"), patch.object(flow, "master_check", return_value=run()), patch.object(flow, "api") as mock_api:
            flow.master_gate("workflow_run", event, "refs/heads/develop")
            self.assertEqual(mock_api.call_args.args[0], "statuses/abc")
            self.assertEqual(mock_api.call_args.args[1]["state"], "success")
        with patch.object(flow, "formal_commit", side_effect=ValueError("Wrong provenance")), patch.object(flow, "api") as mock_api:
            with self.assertRaises(ValueError):
                flow.master_gate("workflow_run", event, "refs/heads/develop")
            mock_api.assert_not_called()
        with patch.object(flow, "formal_commit", return_value="1.2.3"), patch.object(flow, "master_check", side_effect=ValueError("Failed Check")), patch.object(flow, "api") as mock_api:
            with self.assertRaises(ValueError):
                flow.master_gate("workflow_run", event, "refs/heads/develop")
            self.assertEqual(mock_api.call_args.args[1]["state"], "failure")

    def test_tag_rules_require_actions_master_status_on_creation(self):
        rules = json.loads((ROOT / ".github/rulesets/release-tags.json").read_text())
        self.assertEqual(rules["target"], "tag")
        self.assertEqual(rules["bypass_actors"], [])
        self.assertTrue({"update", "deletion"}.issubset({r["type"] for r in rules["rules"]}))
        checks = next(r["parameters"] for r in rules["rules"] if r["type"] == "required_status_checks")
        self.assertFalse(checks["do_not_enforce_on_create"])
        self.assertEqual(checks["required_status_checks"], [{"context": flow.CONTEXT, "integration_id": 15368}])


if __name__ == "__main__":
    unittest.main()
