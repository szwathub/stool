#!/usr/bin/env python3
"""Enforce stool's Git-flow release provenance (Python 3.12+, standard library)."""
import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

REPOSITORY = "szwathub/stool"
CONTEXT = "master-release"
VERSION = r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def api(path, data=None):
    """CI uses its scoped token; local maintainers use gh's existing login."""
    endpoint = f"repos/{REPOSITORY}/{path}"
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        request = urllib.request.Request(
            f"https://api.github.com/{endpoint}",
            data=json.dumps(data).encode() if data is not None else None,
            headers={"Authorization": f"Bearer {token}",
                     "Accept": "application/vnd.github+json",
                     "X-GitHub-Api-Version": "2022-11-28"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    command = ["gh", "api", endpoint]
    if data is not None:
        command += ["--method", "POST", "--input", "-"]
    return json.loads(subprocess.check_output(
        command, input=json.dumps(data) if data is not None else None, text=True))


def paginated(path):
    result = []
    for page in range(1, 101):
        items = api(f"{path}{'&' if '?' in path else '?'}per_page=100&page={page}")
        result.extend(items)
        if len(items) < 100:
            return result
    raise ValueError("Too many API results; refusing an incomplete provenance check")


def metadata(commit):
    package = tomllib.loads(git("show", f"{commit}:Cargo.toml"))["package"]
    version = package["version"]
    require(re.fullmatch(VERSION, version), "Invalid Cargo release version")
    lock = tomllib.loads(git("show", f"{commit}:Cargo.lock"))["package"]
    require(any(p["name"] == "stool" and p["version"] == version for p in lock),
            "Cargo.lock version does not match Cargo.toml")
    notes = git("show", f"{commit}:RELEASE_NOTES.md")
    require(len(re.findall(rf"(?m)^# stool v{re.escape(version)}$", notes)) == 1,
            "Expected exactly one matching release-notes section")
    return version


def source_version(ref):
    match = re.fullmatch(rf"(?:release|hotfix)/v?({VERSION})", ref)
    require(match, "master accepts only release/<version> or hotfix/<version>")
    return match.group(1)


def check_pull_request(pr, version):
    require(pr["head"]["repo"] and pr["head"]["repo"]["full_name"] == REPOSITORY,
            "Release PR must originate in this repository")
    require(source_version(pr["head"]["ref"]) == version,
            "Release/hotfix branch version must match Cargo metadata")


def check_event(event_name, event):
    if event_name != "pull_request":
        return
    pr = event["pull_request"]
    base = pr["base"]["ref"]
    require(base in ("master", "develop"), "PR target must be master or develop")
    if base == "master":
        check_pull_request(pr, metadata("HEAD"))
        # A release starts on develop; a hotfix starts on master. Branch ancestry
        # is retained by the merge-only repository rule.
        if pr["head"]["ref"].startswith("release/"):
            git("merge-base", "--is-ancestor", "origin/master", "HEAD")
    else:
        require(pr["head"]["ref"] == "master" or
                pr["head"]["ref"].startswith(("feature/", "release/", "hotfix/")),
                "develop accepts feature/*, release/*, hotfix/* or master back-merges")


def formal_commit(commit):
    # Ordinary is-ancestor would also accept develop commits merged as a side
    # parent. Only master's first-parent merge commits can be tagged.
    require(commit in git("rev-list", "--first-parent", "origin/master").splitlines(),
            "Release commit must be on master's first-parent history")
    require(len(git("rev-list", "--parents", "-n", "1", commit).split()) == 3,
            "Release must be a two-parent merge commit (no squash/rebase)")
    version = metadata(commit)
    prs = paginated(f"commits/{commit}/pulls")
    matching = [pr for pr in prs if pr.get("merged_at") and
                pr["base"]["ref"] == "master" and pr.get("merge_commit_sha") == commit]
    require(len(matching) == 1, "Commit must merge exactly one release/hotfix PR into master")
    check_pull_request(matching[0], version)
    return version


def master_check(commit):
    data = api(f"actions/workflows/ci.yml/runs?branch=master&event=push&head_sha={commit}&per_page=100")
    runs = [run for run in data["workflow_runs"] if run["head_sha"] == commit and
            run["head_branch"] == "master" and run["event"] == "push"]
    require(runs, "No Check push run exists for this master commit")
    latest = max(runs, key=lambda run: (run["id"], run.get("run_attempt", 1)))
    require(latest["status"] == "completed" and latest["conclusion"] == "success",
            "The latest Check push run for this master commit must succeed")
    return latest


def release_commit(commit):
    version = formal_commit(commit)
    run = master_check(commit)
    return version, run


def dispatch_ref(event_name, ref):
    if event_name == "workflow_dispatch":
        require(ref == "refs/heads/master", "Manual release operations must run from master")


def master_gate(event_name, event, ref):
    dispatch_ref(event_name, ref)
    if event_name == "workflow_run":
        run = event["workflow_run"]
        require(run["event"] == "push" and run["head_branch"] == "master" and
                run["head_repository"]["full_name"] == REPOSITORY,
                "Only this repository's master push Check run may grant release status")
        commit = run["head_sha"]
    else:
        require(event_name == "workflow_dispatch", "Unsupported master gate event")
        commit = git("rev-parse", "origin/master")
    # Grant no status at all on an ineligible commit, including develop commits.
    version = formal_commit(commit)
    try:
        run = master_check(commit)
    except ValueError:
        api(f"statuses/{commit}", {"state": "failure", "context": CONTEXT,
                                 "description": "Master Check has not passed"})
        raise
    api(f"statuses/{commit}", {
        "state": "success", "context": CONTEXT,
        "description": f"Master release v{version}: provenance and Check passed",
        "target_url": run["html_url"],
    })
    print(f"Eligible master release: {commit} (v{version})")


def verify_tag(tag, expected_commit=None, expected_object=None):
    require(re.fullmatch(rf"v{VERSION}", tag), "Expected vMAJOR.MINOR.PATCH release tag")
    ref = api(f"git/ref/tags/{tag}")["object"]
    require(ref["type"] == "tag", "Release requires an annotated, signed tag")
    tag_object = api(f"git/tags/{ref['sha']}")
    require(tag_object["verification"]["verified"], "Tag signature must be verified by GitHub")
    require(tag_object["tag"] == tag and tag_object["object"]["type"] == "commit",
            "Tag must refer directly to a commit")
    commit = tag_object["object"]["sha"]
    require(not expected_commit or commit == expected_commit, "Tag commit changed during release")
    require(not expected_object or ref["sha"] == expected_object, "Tag object changed during release")
    version, _ = release_commit(commit)
    require(tag == "v" + version, "Tag version does not match release commit")
    return {"commit": commit, "tag_object": ref["sha"], "version": version,
            "prerelease": str("-" in version).lower()}


def create_tag(tag, push):
    require(re.fullmatch(rf"v{VERSION}", tag), "Expected vMAJOR.MINOR.PATCH release tag")
    require(git("branch", "--show-current") == "master", "Switch to master before tagging")
    require(not git("status", "--porcelain"), "Working tree must be clean before tagging")
    subprocess.run(["git", "fetch", "origin", "master", "--tags"], check=True)
    commit = git("rev-parse", "HEAD")
    require(commit == git("rev-parse", "origin/master"), "Local master must match origin/master")
    version, _ = release_commit(commit)
    require(tag == "v" + version, "Tag version does not match release commit")
    require(tag not in git("tag", "--list", tag).splitlines(), "Tag already exists; never move a release tag")
    # Let the user's configured GPG signing mechanism request the passphrase.
    subprocess.run(["git", "tag", "--sign", tag, "-m", f"stool {tag}", commit], check=True)
    if push:
        subprocess.run(["git", "push", "origin", f"refs/tags/{tag}"], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check-event")
    sub.add_parser("master-gate")
    verify = sub.add_parser("verify-tag")
    verify.add_argument("--tag", required=True)
    verify.add_argument("--expected-commit")
    verify.add_argument("--expected-object")
    tag = sub.add_parser("tag")
    tag.add_argument("tag")
    tag.add_argument("--push", action="store_true")
    args = parser.parse_args()
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    event = json.loads(Path(event_path).read_text()) if event_path else {}
    ref = os.environ.get("GITHUB_REF", "")
    if args.command == "check-event":
        check_event(event_name, event)
    elif args.command == "master-gate":
        master_gate(event_name, event, ref)
    elif args.command == "verify-tag":
        dispatch_ref(event_name, ref)
        require(event_name in ("push", "workflow_dispatch", ""), "Unsupported release event")
        if event_name == "push":
            require(ref == f"refs/tags/{args.tag}", "Push ref must match the release tag")
        values = verify_tag(args.tag, args.expected_commit, args.expected_object)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.writelines(f"{key}={value}\n" for key, value in values.items())
        print(json.dumps(values))
    else:
        create_tag(args.tag, args.push)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, subprocess.CalledProcessError, urllib.error.URLError) as error:
        sys.exit(f"Git-flow policy: {error}")
