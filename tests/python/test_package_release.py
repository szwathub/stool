"""Check release packaging integration and run the published checksum step."""
import hashlib
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import textwrap
import tomllib
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / '.github/workflows/release.yml').read_text()
TARGETS = (
    'x86_64-unknown-linux-gnu',
    'x86_64-unknown-linux-musl',
    'aarch64-unknown-linux-gnu',
    'x86_64-apple-darwin',
    'aarch64-apple-darwin',
    'x86_64-pc-windows-msvc',
)


def job(name):
    return re.search(rf'(?ms)^  {name}:\n(.*?)(?=^  [\w-]+:\n|\Z)', WORKFLOW)[1]


def step(name, block=WORKFLOW):
    return block.split(f'      - name: {name}\n', 1)[1].split('\n      - ', 1)[0]


def fields(block, indentation):
    """Read scalar fields and literal blocks at one known workflow indentation."""
    result = {}
    lines = block.splitlines()
    for index, line in enumerate(lines):
        match = re.fullmatch(r' ' * indentation + r'([\w-]+):(?: (.*))?', line)
        if not match:
            continue
        key, value = match.groups()
        if value == '|':
            body = []
            for child in lines[index + 1:]:
                if child and not child.startswith(' ' * (indentation + 2)):
                    break
                body.append(child)
            value = textwrap.dedent('\n'.join(body)).rstrip()
        result[key] = (value or '').strip('\'"')
    return result


class PackagingIntegrationTests(unittest.TestCase):
    def test_action_builds_the_checked_out_commit_once_and_preserves_archive_layout(self):
        build = job('build')
        package = step('Build and package release', build)
        self.assertIn('ref: ${{ needs.style.outputs.commit }}', build)
        self.assertEqual(fields(package, 8)['id'], 'package')
        self.assertEqual(fields(package, 8)['uses'], 'taiki-e/upload-rust-binary-action@v1')
        inputs = fields(package, 10)
        expected = {
            'bin': 'stool',
            'target': '${{ matrix.target }}',
            'archive': 'stool-$tag-$target',
            'ref': 'refs/tags/${{ env.RELEASE_TAG }}',
            'leading-dir': 'true',
            'include': "README.md,LICENSE,completions${{ runner.os != 'Windows' && ',man-page' || '' }}",
            'locked': 'true',
            'build-tool': 'cargo',
            'tar': 'unix',
            'zip': 'windows',
            'dry-run': 'true',
            'dry-run-intended': 'true',
        }
        for name, value in expected.items():
            with self.subTest(input=name):
                self.assertEqual(inputs[name], value)
        self.assertNotIn('checksum', inputs, 'Checksums are created once in publish')
        self.assertNotIn('token', inputs, 'Matrix builds must not publish releases')
        self.assertEqual(build.count('uses: taiki-e/upload-rust-binary-action@'), 1)
        self.assertNotRegex(build, r'\bcargo(?:\s+\+\S+)?\s+build\b')
        self.assertNotIn('name: Package release', build)
        targets = re.findall(r'^            target: (\S+)$', build, re.MULTILINE)
        self.assertCountEqual(targets, TARGETS)

    def test_archives_are_uploaded_after_packaging_and_published_after_all_builds(self):
        build = job('build')
        upload = step('Upload release archive', build)
        self.assertEqual(fields(upload, 8)['uses'], 'actions/upload-artifact@v4')
        self.assertEqual(fields(upload, 10)['path'].splitlines(), [
            '${{ steps.package.outputs.tar }}', '${{ steps.package.outputs.zip }}',
        ])
        self.assertEqual(fields(upload, 10)['name'], 'stool-${{ matrix.target }}')
        self.assertEqual(fields(upload, 10)['if-no-files-found'], 'error')
        self.assertNotIn('if:', upload, 'Uploading must retain the default success gate')
        self.assertLess(build.index('name: Build and package release'),
                        build.index('name: Upload release archive'))
        self.assertNotIn('contents: write', build)
        self.assertIn('permissions:\n  contents: read', WORKFLOW)
        publish = job('publish')
        self.assertEqual(fields(publish, 4)['needs'], '[style, build]')
        self.assertNotIn('if:', publish)
        self.assertEqual(WORKFLOW.count('uses: softprops/action-gh-release@'), 1)
        self.assertIn('uses: softprops/action-gh-release@v2', publish)
        self.assertIn('files: artifacts/*', publish)
        self.assertIn('contents: write', publish)

    def test_debian_assets_use_the_project_root_directories(self):
        metadata = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['metadata']['deb']
        sources = [entry[0] for entry in metadata['assets']]
        self.assertIn('man-page/*.1', sources)
        self.assertIn('completions/stool.bash', sources)


@unittest.skipUnless(shutil.which('bash') and shutil.which('sha256sum'),
                     'The Linux checksum step requires Bash and sha256sum')
class ChecksumTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='stool release checksum ')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.output = self.root / 'artifacts'
        self.command = fields(step('Verify archives and create checksums'), 8)['run']

    def archives(self):
        if self.output.exists():
            shutil.rmtree(self.output)
        self.output.mkdir()
        for target in TARGETS:
            basename = f'stool-v1.2.3-{target}'
            windows = target.endswith('-windows-msvc')
            files = {
                'stool.exe' if windows else 'stool': f'binary for {target}'.encode(),
                'README.md': b'Project documentation',
                'LICENSE': b'License',
                'completions/stool.bash': b'Generated Bash completion',
            }
            if windows:
                with zipfile.ZipFile(self.output / (basename + '.zip'), 'w') as archive:
                    for name, data in files.items():
                        archive.writestr(f'{basename}/{name}', data)
            else:
                files['man-page/stool.1'] = b'Generated manual'
                with tarfile.open(self.output / (basename + '.tar.gz'), 'w:gz') as archive:
                    for name, data in files.items():
                        member = tarfile.TarInfo(f'{basename}/{name}')
                        member.size = len(data)
                        archive.addfile(member, io.BytesIO(data))

    def run_checksums(self, ok=True):
        result = subprocess.run([shutil.which('bash'), '-e', '-o', 'pipefail', '-c', self.command],
                                cwd=self.root, env=dict(os.environ, RELEASE_TAG='v1.2.3', LC_ALL='C'),
                                capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, ok, result.stderr)
        return result

    def test_six_archives_produce_sorted_installer_compatible_sha256_checksums(self):
        self.archives()
        expected = ''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n'
                           for path in sorted(self.output.iterdir()))
        result = self.run_checksums()
        self.assertEqual((self.output / 'SHA256SUMS.txt').read_text(), expected)
        self.assertEqual(result.stdout.count(': OK'), len(TARGETS))

    def test_missing_empty_misnamed_or_extra_archive_is_rejected_before_checksums(self):
        for problem in ('missing', 'empty', 'misnamed', 'extra'):
            with self.subTest(problem=problem):
                self.archives()
                archive = self.output / f'stool-v1.2.3-{TARGETS[0]}.tar.gz'
                if problem == 'missing':
                    archive.unlink()
                elif problem == 'empty':
                    archive.write_bytes(b'')
                elif problem == 'misnamed':
                    archive.rename(self.output / 'wrong-version.tar.gz')
                else:
                    (self.output / 'unexpected.zip').write_bytes(b'Unexpected archive')
                self.run_checksums(ok=False)
                self.assertFalse((self.output / 'SHA256SUMS.txt').exists())


if __name__ == '__main__':
    unittest.main()
