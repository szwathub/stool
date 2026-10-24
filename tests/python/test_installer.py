"""Offline installer checks: no Git, network access, or writes outside a temporary HOME."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
VERSION = 'v' + tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
DOWNLOAD = r'''
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
url = next(arg for arg in args if arg.startswith('https://'))
with open(os.environ['INSTALL_TEST_LOG'], 'a') as stream:
    stream.write(json.dumps(url) + '\n')
if '--write-out' in args:
    assert url == 'https://github.com/szwathub/stool/releases/latest'
    print('https://github.com/szwathub/stool/releases/tag/' + os.environ.get('INSTALL_TEST_LATEST_VERSION', os.environ['INSTALL_TEST_RELEASE_VERSION']), end='')
    raise SystemExit(0)
if '--spider' in args:
    print('  Location: https://github.com/szwathub/stool/releases/tag/' + os.environ.get('INSTALL_TEST_LATEST_VERSION', os.environ['INSTALL_TEST_RELEASE_VERSION']) + ' [following]', file=sys.stderr)
    raise SystemExit(0)
if '--output' in args:
    output = args[args.index('--output') + 1]
else:
    output = next(arg.split('=', 1)[1] for arg in args if arg.startswith('--output-document='))
name = url.rsplit('/', 1)[1]
if os.environ.get('INSTALL_TEST_FAIL') == name:
    raise SystemExit(22)
shutil.copyfile(pathlib.Path(os.environ['INSTALL_TEST_ASSETS']) / name, output)
'''


def executable(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(0o755)


@unittest.skipIf(os.name == 'nt', 'Unix Bash installer; Windows assets are checked with a simulated Git Bash')
class InstallerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='stool-installer-test-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.home = self.root / 'home'
        self.home.mkdir()
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.assets = self.root / 'assets'
        self.assets.mkdir()
        self.tmp = self.root / 'tmp'
        self.tmp.mkdir()
        self.log = self.root / 'download-log'
        self.env = dict(os.environ, HOME=str(self.home), PATH=str(self.bin),
                        TMPDIR=str(self.tmp), VERSION=VERSION,
                        INSTALL_TEST_LOG=str(self.log), INSTALL_TEST_ASSETS=str(self.assets), INSTALL_TEST_RELEASE_VERSION=VERSION)
        self.env.pop('PREFIX', None)
        self.env.pop('INSTALL_TEST_FAIL', None)
        # GNU tar invokes gzip externally for -xzf; expose that dependency too.
        for name in ['mktemp', 'mkdir', 'rm', 'awk', 'chmod', 'cp', 'mv', 'tar', 'gzip', 'unzip', 'sha256sum', 'shasum']:
            tool = shutil.which(name)
            if tool:
                (self.bin / name).symlink_to(tool)
        executable(self.bin / 'uname', '#!/bin/sh\ncase "$1" in -s) printf "%s\\n" "$INSTALL_TEST_OS";; -m) printf "%s\\n" "$INSTALL_TEST_ARCH";; *) exit 1;; esac\n')
        executable(self.bin / 'ldd', '#!/bin/sh\nprintf "%s\\n" "$INSTALL_TEST_LIBC"\nexit 1\n')
        executable(self.bin / 'curl', '#!' + sys.executable + '\n' + DOWNLOAD)
        self.platform('Darwin', 'arm64')

    def platform(self, system, arch, libc='glibc'):
        self.env.update(INSTALL_TEST_OS=system, INSTALL_TEST_ARCH=arch, INSTALL_TEST_LIBC=libc)

    def fixture(self, target='aarch64-apple-darwin', checksum='valid', executable_version=None, version=VERSION):
        if executable_version is None:
            executable_version = version[1:]
        base = 'stool-' + version + '-' + target
        windows = target.endswith('windows-msvc')
        name = base + ('.zip' if windows else '.tar.gz')
        program = ('#!/bin/sh\nprintf "stool ' + executable_version + '\\n"\n').encode()
        if windows:
            with zipfile.ZipFile(self.assets / name, 'w') as archive:
                archive.writestr(base + '/stool.exe', program)
        else:
            with tarfile.open(self.assets / name, 'w:gz') as archive:
                info = tarfile.TarInfo(base + '/stool')
                info.size = len(program)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(program))
        digest = hashlib.sha256((self.assets / name).read_bytes()).hexdigest()
        line = digest + '  ' + name + '\n'
        if checksum == 'mismatch':
            line = '0' * 64 + '  ' + name + '\n'
        elif checksum == 'missing':
            line = digest + '  another-asset.tar.gz\n'
        elif checksum == 'duplicate':
            line *= 2
        elif checksum == 'malformed':
            line = 'invalid  ' + name + '\n'
        (self.assets / 'SHA256SUMS.txt').write_text(line)
        return name

    def run_installer(self, ok=True):
        result = subprocess.run(['/bin/bash', str(ROOT / 'install.sh')], env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
        self.assertEqual(list(self.tmp.iterdir()), [], 'download staging directory must be cleaned up')
        return result

    def downloads(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def existing_binary(self):
        path = Path(self.env.get('PREFIX', self.home / '.local')) / 'bin/stool'
        executable(path, 'existing binary\n')
        return path

    def test_fresh_machine_without_git_installs_verified_release(self):
        name = self.fixture()
        self.assertIsNone(shutil.which('git', path=self.env['PATH']))
        self.env['VERSION'] = VERSION[1:]
        self.run_installer()
        binary = self.home / '.local/bin/stool'
        result = subprocess.check_output([str(binary), '--version'], text=True)
        self.assertEqual(result, 'stool ' + VERSION[1:] + '\n')
        self.assertEqual(stat.S_IMODE(binary.stat().st_mode), 0o755)
        self.assertEqual(self.downloads(), [
            'https://github.com/szwathub/stool/releases/download/' + VERSION + '/' + name,
            'https://github.com/szwathub/stool/releases/download/' + VERSION + '/SHA256SUMS.txt',
        ])
        self.assertEqual(list(binary.parent.glob('.stool-install.*')), [])

    def test_custom_prefix_installs_without_modifying_shell_configuration(self):
        self.fixture()
        self.env['PREFIX'] = str(self.root / 'custom prefix')
        self.run_installer()
        self.assertTrue((self.root / 'custom prefix/bin/stool').is_file())
        # macOS may create Library while running downloaded executables. Check
        # the installer's promised boundaries rather than OS-managed entries.
        for relative in ['.bashrc', '.bash_profile', '.profile', '.zshrc', '.zprofile',
                         '.config/fish/config.fish', '.local/bin/stool']:
            self.assertFalse((self.home / relative).exists(), relative)

    def test_latest_uses_the_release_redirect_without_a_github_api_call(self):
        self.fixture()
        self.env.pop('VERSION')
        self.run_installer()
        self.assertEqual(self.downloads()[0], 'https://github.com/szwathub/stool/releases/latest')
        self.assertFalse(any('api.github.com' in url for url in self.downloads()))

    def test_wget_can_install_latest_without_curl(self):
        self.fixture()
        (self.bin / 'curl').unlink()
        executable(self.bin / 'wget', '#!' + sys.executable + '\n' + DOWNLOAD)
        self.env['VERSION'] = 'latest'
        self.run_installer()
        self.assertTrue((self.home / '.local/bin/stool').exists())

    def test_explicit_stable_versions_are_not_tied_to_a_previous_numbering_scheme(self):
        for version in ('v0.1.0', 'v1.0.0'):
            with self.subTest(version=version):
                self.fixture(version=version)
                self.env['VERSION'] = version
                self.run_installer()
                binary = self.home / '.local/bin/stool'
                self.assertEqual(subprocess.check_output([str(binary), '--version'], text=True),
                                 'stool ' + version[1:] + '\n')
                self.assertIn('/' + version + '/', self.downloads()[-2])

    def test_latest_respects_the_resolved_stable_version_without_a_historical_floor(self):
        self.fixture(version='v0.1.0')
        self.env['VERSION'] = 'latest'
        self.env['INSTALL_TEST_LATEST_VERSION'] = 'v0.1.0'
        self.run_installer()
        binary = self.home / '.local/bin/stool'
        self.assertEqual(subprocess.check_output([str(binary), '--version'], text=True), 'stool 0.1.0\n')
        self.assertEqual(self.downloads()[0], 'https://github.com/szwathub/stool/releases/latest')
        self.assertIn('/v0.1.0/', self.downloads()[-2])

    def test_invalid_or_prerelease_versions_never_download_or_overwrite(self):
        binary = self.existing_binary()
        for version in ('v1.2.3-rc.1', 'v1.2.3+build', '../release', 'v1.2',
                        'v01.2.3', 'v1.2.3/extra', 'v1.2.3\n'):
            with self.subTest(version=version):
                self.env['VERSION'] = version
                result = self.run_installer(ok=False)
                self.assertIn('VERSION must be latest or vMAJOR.MINOR.PATCH', result.stderr)
                self.assertEqual(self.downloads(), [])
                self.assertEqual(binary.read_text(), 'existing binary\n')

    def test_latest_rejects_prereleases_before_downloading_a_package(self):
        binary = self.existing_binary()
        self.env['VERSION'] = 'latest'
        self.env['INSTALL_TEST_LATEST_VERSION'] = 'v1.0.0-rc.1'
        result = self.run_installer(ok=False)
        self.assertIn('stable releases only', result.stderr)
        self.assertEqual(self.downloads(), ['https://github.com/szwathub/stool/releases/latest'])
        self.assertEqual(binary.read_text(), 'existing binary\n')

    def test_target_selection_matches_all_release_assets(self):
        cases = [
            ('Darwin', 'arm64', 'glibc', 'aarch64-apple-darwin'),
            ('Darwin', 'x86_64', 'glibc', 'x86_64-apple-darwin'),
            ('Linux', 'x86_64', 'glibc', 'x86_64-unknown-linux-gnu'),
            ('Linux', 'x86_64', 'musl', 'x86_64-unknown-linux-musl'),
            ('Linux', 'aarch64', 'glibc', 'aarch64-unknown-linux-gnu'),
            ('MINGW64_NT', 'x86_64', 'glibc', 'x86_64-pc-windows-msvc'),
        ]
        for system, arch, libc, target in cases:
            with self.subTest(target=target):
                if target.endswith('windows-msvc') and not (self.bin / 'unzip').exists():
                    continue
                self.platform(system, arch, libc)
                name = self.fixture(target)
                self.run_installer()
                self.assertTrue(self.downloads()[-2].endswith('/' + name))

    def test_bad_or_missing_checksums_preserve_the_existing_binary(self):
        binary = self.existing_binary()
        for mode in ['mismatch', 'missing', 'duplicate', 'malformed']:
            with self.subTest(checksum=mode):
                self.fixture(checksum=mode)
                self.run_installer(ok=False)
                self.assertEqual(binary.read_text(), 'existing binary\n')
                self.assertEqual(list(binary.parent.glob('.stool-install.*')), [])

    def test_download_failure_preserves_the_existing_binary(self):
        binary = self.existing_binary()
        name = self.fixture()
        for failed in [name, 'SHA256SUMS.txt']:
            with self.subTest(download=failed):
                self.env['INSTALL_TEST_FAIL'] = failed
                self.run_installer(ok=False)
                self.assertEqual(binary.read_text(), 'existing binary\n')

    def test_wrong_executable_version_is_not_installed(self):
        binary = self.existing_binary()
        self.fixture(executable_version='0.2.3')
        result = self.run_installer(ok=False)
        self.assertIn('Executable version does not match', result.stderr)
        self.assertEqual(binary.read_text(), 'existing binary\n')

    def test_missing_hash_tool_stops_before_downloading(self):
        self.fixture()
        for name in ['sha256sum', 'shasum']:
            path = self.bin / name
            if path.exists():
                path.unlink()
        result = self.run_installer(ok=False)
        self.assertIn('SHA256 verification is required', result.stderr)
        self.assertEqual(self.downloads(), [])

    def test_unsupported_targets_stop_before_downloading(self):
        for system, arch, libc in [('Linux', 'aarch64', 'musl'), ('Darwin', 'i686', 'glibc'),
                                  ('MINGW64_NT', 'aarch64', 'glibc')]:
            with self.subTest(system=system, arch=arch, libc=libc):
                self.platform(system, arch, libc)
                self.run_installer(ok=False)
                self.assertEqual(self.downloads(), [])


if __name__ == '__main__':
    unittest.main()
