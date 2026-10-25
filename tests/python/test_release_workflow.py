"""Check tag-based release workflow dependencies and configuration."""
import json
from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / '.github/workflows/release.yml').read_text()
CI_WORKFLOW = (ROOT / '.github/workflows/style.yml').read_text()


def job(name, workflow=WORKFLOW):
    return re.search(rf'(?ms)^  {name}:\n(.*?)(?=^  [\w-]+:\n|\Z)', workflow)[1]


class ReleaseWiringTests(unittest.TestCase):
    def test_tag_checkout_pins_style_builds_and_publication_to_one_commit(self):
        style = job('style')
        self.assertIn('commit: ${{ steps.checkout.outputs.commit }}', style)
        self.assertIn('name: Check out release tag', style)
        self.assertIn('id: checkout', style)
        self.assertIn('ref: refs/tags/${{ env.RELEASE_TAG }}', style)
        self.assertEqual(style.count('uses: actions/checkout@v4'), 1)
        self.assertLess(style.index('name: Check out release tag'),
                        style.index('name: Check formatting'))
        self.assertIn('needs: style', job('build'))
        for name in ('build', 'publish'):
            self.assertIn('ref: ${{ needs.style.outputs.commit }}', job(name))

    def test_style_checks_cover_three_platforms_without_running_rust_tests(self):
        style = job('style')
        self.assertIn('runs-on: ${{ matrix.os }}', style)
        platforms = re.search(r'^        os: \[(.*?)\]$', style, re.MULTILINE)[1]
        self.assertCountEqual([item.strip() for item in platforms.split(',')],
                              ['ubuntu-22.04', 'macos-14', 'windows-2022'])
        self.assertIn('fail-fast: false', style)
        self.assertIn('key: release-style-${{ matrix.os }}', style)
        steps = style.split('\n      - ')
        stable = next(step for step in steps if 'uses: dtolnay/rust-toolchain@stable' in step)
        self.assertIn('components: rustfmt, clippy', stable)
        spelling = next(step for step in steps if 'name: Check spelling' in step)
        self.assertIn('uses: crate-ci/typos@v1.28.4', spelling)
        wget = next(step for step in steps if 'choco install wget --no-progress' in step)
        self.assertIn("if: runner.os == 'Windows'", wget)
        self.assertIn('cargo fmt --all -- --check', style)
        clippy = next(step for step in steps if 'name: Check Clippy' in step)
        self.assertIn('if: success() || failure()', clippy)
        self.assertIn('cargo clippy --locked --all-targets -- -D warnings', clippy)
        for workflow in (ROOT / '.github/workflows').glob('*.yml'):
            toolchains = re.findall(r'uses: dtolnay/rust-toolchain@(\S+)',
                                    workflow.read_text())
            self.assertTrue(toolchains, workflow.name)
            self.assertEqual(set(toolchains), {'stable'}, workflow.name)
        self.assertEqual(style.count('uses: dtolnay/rust-toolchain@stable'), 1)
        self.assertNotRegex(WORKFLOW, r'\bcargo\s+\+\S+')
        manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(manifest['package']['edition'], '2021')
        self.assertNotIn('rust-version', manifest['package'])
        self.assertNotRegex(WORKFLOW, r'\bcargo(?:\s+\+\S+)?\s+test\b')

    def test_develop_push_uses_release_style_checks_on_the_pushed_commit(self):
        self.assertTrue(CI_WORKFLOW.startswith('name: Style\n'))
        self.assertIn('push:\n    branches: [develop]', CI_WORKFLOW)
        self.assertEqual({path.name for path in (ROOT / '.github/workflows').glob('*.yml')},
                         {'style.yml', 'release.yml'})
        self.assertNotIn('pull_request:', CI_WORKFLOW)
        self.assertNotIn('workflow_dispatch:', CI_WORKFLOW)
        self.assertIn('group: style-${{ github.ref }}', CI_WORKFLOW)
        self.assertIn('cancel-in-progress: true', CI_WORKFLOW)
        self.assertEqual(re.findall(r'^  ([\w-]+):\n    name:', CI_WORKFLOW,
                                    re.MULTILINE), ['style'])
        style = job('style', CI_WORKFLOW)
        self.assertIn('name: Style (${{ matrix.os }})', style)
        self.assertIn('runs-on: ${{ matrix.os }}', style)
        matrix = re.search(r'^        os: \[(.*?)\]$', style, re.MULTILINE)[1]
        self.assertEqual(matrix, re.search(r'^        os: \[(.*?)\]$', job('style'),
                                           re.MULTILINE)[1])
        self.assertIn('fail-fast: false', style)
        self.assertIn('key: check-style-${{ matrix.os }}', style)
        self.assertIn('uses: actions/checkout@v4', style)
        self.assertIn('persist-credentials: false', style)
        self.assertNotRegex(style, r'(?m)^\s+(?:ref|outputs):')
        self.assertNotIn('RELEASE_TAG', CI_WORKFLOW)
        steps = style.split('\n      - ')
        for name in ('Install Rust toolchain', 'Install wget for Windows',
                     'Check spelling', 'Check formatting', 'Check Clippy'):
            ci_step = next(step for step in steps if f'name: {name}\n' in step)
            release_step = next(step for step in job('style').split('\n      - ')
                                if f'name: {name}\n' in step)
            self.assertEqual(ci_step.strip(), release_step.strip(), name)
        self.assertNotRegex(CI_WORKFLOW, r'\bcargo(?:\s+\+\S+)?\s+(?:build|test)\b')

    def test_automatic_manual_and_prerelease_paths_share_the_release_tag(self):
        self.assertIn('tags: ["v*.*.*"]', WORKFLOW)
        self.assertIn('workflow_dispatch:\n    inputs:\n      tag:', WORKFLOW)
        self.assertIn('RELEASE_TAG: ${{ inputs.tag || github.ref_name }}', WORKFLOW)
        publish = job('publish')
        self.assertIn('tag_name: ${{ env.RELEASE_TAG }}', publish)
        self.assertIn("prerelease: ${{ contains(env.RELEASE_TAG, '-') }}", publish)
        self.assertIn("make_latest: ${{ !contains(env.RELEASE_TAG, '-') }}", publish)

    def test_drafter_only_renders_notes_for_the_pinned_release_before_publication(self):
        publish = job('publish')
        steps = publish.split('\n      - ')
        drafter = next(step for step in steps
                       if 'uses: release-drafter/release-drafter@v7' in step)
        release = next(step for step in steps
                       if 'uses: softprops/action-gh-release@v2' in step)
        self.assertIn('id: notes', drafter)
        self.assertIn('config-name: file:release-drafter.yml', drafter)
        self.assertIn('tag: ${{ env.RELEASE_TAG }}', drafter)
        self.assertIn('commitish: ${{ needs.style.outputs.commit }}', drafter)
        self.assertIn('dry-run: true', drafter)
        self.assertIn('body: ${{ steps.notes.outputs.body }}', release)
        self.assertIn('generate_release_notes: false', release)
        self.assertLess(publish.index('Verify archives and create checksums'),
                        publish.index('uses: release-drafter/release-drafter@v7'))
        self.assertLess(steps.index(drafter), steps.index(release))
        self.assertIn('pull-requests: read', publish)
        self.assertNotIn('pull-requests: write', WORKFLOW)
        self.assertNotIn('body_path:', WORKFLOW)
        self.assertNotIn('actions/setup-python@', WORKFLOW)
        self.assertNotRegex(publish, r'\bpython(?:3)?\b')

    def test_notes_use_a_previous_stable_release_even_for_prereleases_and_retries(self):
        drafter = next(step for step in job('publish').split('\n      - ')
                       if 'uses: release-drafter/release-drafter@v7' in step)
        self.assertIn("filter-by-range: '<${{ env.RELEASE_TAG }}'", drafter)
        self.assertNotRegex(drafter, r'(?m)^\s+prerelease(?:-identifier)?:')
        config = (ROOT / '.github/release-drafter.yml').read_text()
        self.assertIn('include-pre-releases: false', config)
        self.assertNotRegex(config, r'(?m)^prerelease(?:-identifier)?:')
        self.assertIn("name-template: 'v$RESOLVED_VERSION'", config)
        self.assertIn("tag-template: 'v$RESOLVED_VERSION'", config)
        self.assertIn('  $CHANGES', config)
        self.assertIn('  $NEW_CONTRIBUTORS', config)
        self.assertIn('https://github.com/$OWNER/$REPOSITORY/compare/'
                      '$PREVIOUS_TAG...v$RESOLVED_VERSION', config)
        self.assertIn("  - type: 'pre-exclude'\n    when:\n"
                      "      label: 'skip-changelog'", config)

    def test_rulesets_keep_pr_and_tag_protection_without_post_merge_check_gates(self):
        for branch in ('develop', 'master'):
            rules = json.loads((ROOT / f'.github/rulesets/{branch}.json').read_text())
            self.assertEqual(rules['enforcement'], 'active')
            self.assertEqual(rules['conditions']['ref_name']['include'], [f'refs/heads/{branch}'])
            self.assertEqual(rules['bypass_actors'], [])
            self.assertTrue({'deletion', 'non_fast_forward', 'pull_request'}.issubset(
                {rule['type'] for rule in rules['rules']}))
            self.assertNotIn('required_status_checks',
                             {rule['type'] for rule in rules['rules']})
            pull_request = next(rule['parameters'] for rule in rules['rules']
                                if rule['type'] == 'pull_request')
            self.assertEqual(pull_request['allowed_merge_methods'], ['merge'])
            self.assertTrue(pull_request['required_review_thread_resolution'])
        tags = json.loads((ROOT / '.github/rulesets/release-tags.json').read_text())
        self.assertEqual(tags['target'], 'tag')
        self.assertEqual(tags['enforcement'], 'active')
        self.assertEqual(tags['bypass_actors'], [])
        self.assertEqual(tags['conditions']['ref_name']['include'], ['refs/tags/v*'])
        self.assertEqual({rule['type'] for rule in tags['rules']}, {'update', 'deletion'})


if __name__ == '__main__':
    unittest.main()
