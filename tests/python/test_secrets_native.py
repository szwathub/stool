"""Disposable native key backup/restore tests; never read the user's keyring."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


def groups(listing):
    result, primary, pending = {}, None, None
    for line in listing.decode().splitlines():
        fields = line.split(':')
        if fields[0] in ('sec', 'pub', 'ssb', 'sub'):
            pending = fields[0]
        elif fields[0] == 'fpr' and pending:
            if pending in ('sec', 'pub'):
                primary = fields[9]
                result[primary] = []
            else:
                result[primary].append(fields[9])
            pending = None
    return {key: sorted(value) for key, value in result.items()}


class NativeSecretsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if os.name != 'posix':
            raise unittest.SkipTest('Disposable native GPG/OpenSSH suite uses Unix keyrings')
        binary = Path(os.environ.get('STOOL_BIN', str(Path(__file__).resolve().parents[2] / 'target/debug/stool')))
        if not binary.is_file() or not shutil.which('gpg') or not shutil.which('ssh-keygen'):
            raise unittest.SkipTest('Build stool and install GPG/OpenSSH')
        cls.binary = str(binary.resolve())

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='stool-native-secrets-', dir='/private/tmp' if os.path.isdir('/private/tmp') else '/tmp')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source, self.target = self.root / 'source', self.root / 'target'
        self.source.mkdir(mode=0o700)
        # A replaced HOME can still map to the UID's shared default agent socket on Linux.
        # A non-default keyring forces a unique, path-derived socket namespace.
        self.source_keyring = self.root / 'source-keyring'
        self.source_keyring.mkdir(mode=0o700)
        (self.source / '.ssh').mkdir(mode=0o700)
        self.env = dict(os.environ, HOME=str(self.source), USERPROFILE=str(self.source), GNUPGHOME=str(self.source_keyring))
        for name in ('GPG_AGENT_INFO', 'GPG_TTY', 'SSH_AUTH_SOCK', 'SSH_AGENT_PID', 'STOOL_AGE_PASSPHRASE', 'GIT_DIR', 'GIT_WORK_TREE'):
            self.env.pop(name, None)
        self.password = self.root / 'password'
        self.password.write_text("synthetic's archive password\n")
        self.password.chmod(0o600)
        self.archive = self.root / 'keys.tar.age'
        self.addCleanup(self.kill, self.source_keyring)
        self.addCleanup(self.kill, self.target / '.gnupg')

    def kill(self, home):
        if shutil.which('gpgconf'):
            subprocess.run(['gpgconf', '--homedir', str(home), '--kill', 'all'], env=dict(self.env, GNUPGHOME=str(home)), capture_output=True, timeout=15)

    def gpg(self, root, *args, input=None, ok=True):
        home = self.source_keyring if root == self.source else root / '.gnupg'
        result = subprocess.run(['gpg', '--no-options', '--homedir', str(home), '--batch', '--pinentry-mode', 'loopback', *args],
                                env=dict(self.env, GNUPGHOME=str(home)), input=input, capture_output=True, timeout=40)
        self.assertEqual(result.returncode == 0, ok, result.stderr.decode(errors='replace'))
        return result.stdout

    def generate(self, name, encryption=True):
        self.gpg(self.source, '--passphrase', '', '--quick-generate-key', name, 'ed25519', 'sign', '0')
        fingerprint = next(iter(groups(self.gpg(self.source, '--with-colons', '--fingerprint', '--fingerprint', '--list-secret-keys', name))))
        if encryption:
            self.gpg(self.source, '--passphrase', '', '--quick-add-key', fingerprint, 'cv25519', 'encr', '0')
        return fingerprint

    def ssh(self, name='github_rsa', password=''):
        path = self.source / '.ssh' / name
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', password, '-C', 'stool-native@example.invalid', '-f', str(path)],
                       env=self.env, capture_output=True, check=True, timeout=30)
        return path

    def run_secrets(self, *args, ok=True, env=None):
        result = subprocess.run([self.binary, 'secrets', *map(str, args), '--passphrase-file', str(self.password)],
                                cwd=self.root, env=env or self.env, capture_output=True, timeout=90)
        self.assertEqual(result.returncode == 0, ok, result.stderr.decode(errors='replace'))
        self.assertNotIn(self.password.read_bytes().strip(), result.stdout + result.stderr)
        self.assertNotIn(b'BEGIN OPENSSH PRIVATE KEY', result.stdout + result.stderr)
        self.assertNotIn(b'BEGIN PGP PRIVATE KEY', result.stdout + result.stderr)
        return result

    def backup(self, *selection):
        return self.run_secrets('backup', *selection, '--output', self.archive)

    def restore(self, *selection, ok=True):
        return self.run_secrets('restore', *selection, '--input', self.archive, '--output', self.target, ok=ok)

    def sign_and_decrypt(self, keys):
        actual = groups(self.gpg(self.target, '--with-colons', '--fingerprint', '--fingerprint', '--list-secret-keys'))
        original = groups(self.gpg(self.source, '--with-colons', '--fingerprint', '--fingerprint', '--list-secret-keys'))
        self.assertEqual(actual, original)
        self.assertEqual(sorted(actual), sorted(keys))
        for key in keys:
            signature = self.gpg(self.target, '--passphrase', '', '--local-user', key, '--clearsign', input=b'key restore signature proof\n')
            status = self.gpg(self.target, '--status-fd', '1', '--verify', input=signature)
            self.assertIn(b'VALIDSIG ', status)
            ciphertext = self.gpg(self.source, '--trust-model', 'always', '--recipient', key, '--encrypt', input=b'key restore decryption proof\n')
            self.assertEqual(self.gpg(self.target, '--passphrase', '', '--decrypt', input=ciphertext), b'key restore decryption proof\n')
        origin_trust = self.gpg(self.source, '--export-ownertrust')
        target_trust = self.gpg(self.target, '--export-ownertrust')
        filtered = lambda value: sorted(line for line in value.splitlines() if line and not line.startswith(b'#'))
        self.assertEqual(filtered(origin_trust), filtered(target_trust))

    @unittest.skipUnless(sys.platform == 'linux', 'macOS Apple stages use injected unit-test executors; do not change real preferences')
    def test_init_without_gpg_restores_ssh_then_standalone_restore_recovers_personal_and_work(self):
        personal = self.generate('Personal <personal@example.invalid>')
        work = self.generate('Work <work@example.invalid>')
        identity = self.ssh()
        self.backup()
        repository = self.root / 'dotfiles'
        vendor = self.root / 'vendor'
        git_env = dict(self.env, GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1', GIT_ALLOW_PROTOCOL='file')
        def git(directory, *args):
            subprocess.run(['git', '-C', str(directory), '-c', 'user.name=Fixture', '-c',
                            'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', *args],
                           env=git_env, capture_output=True, check=True, timeout=30)
        def commit(directory):
            git(directory, 'add', '.')
            git(directory, 'commit', '-m', 'fixture')
        for directory in (vendor, repository):
            directory.mkdir()
            git(directory, 'init', '-b', 'main')
        (vendor / 'version').write_text('vendor fixture\n')
        commit(vendor)
        (repository / 'dot_settings').write_text('fixture configuration v1\n')
        (repository / 'dot_Brewfile').write_text('cask "gpg-suite"\n')
        git(repository, 'submodule', 'add', vendor.as_uri(), 'dot_vendor')
        commit(repository)
        empty_path = self.root / 'empty-path'
        empty_path.mkdir()
        env = dict(git_env, HOME=str(self.target), USERPROFILE=str(self.target), PATH=str(empty_path),
                   GNUPGHOME=str(self.target / '.gnupg'))
        result = subprocess.run([self.binary, 'init', '--dotfile-url', repository.as_uri(),
                                 '--key-file', str(self.archive), '--passphrase-file', str(self.password)],
                                cwd=self.root, env=env, capture_output=True, timeout=90)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
        self.assertIn(b'Pending: GPG', result.stdout)
        self.assertNotIn(self.password.read_bytes().strip(), result.stdout + result.stderr)
        self.assertEqual((self.target / '.ssh/github_rsa').read_bytes(), identity.read_bytes())
        self.assertFalse((self.target / '.gnupg').exists())
        self.assertFalse((self.target / '.stool/dotfile').exists())
        config = (self.target / '.stool/config.toml').read_text()
        self.assertIn(repository.as_uri(), config)
        self.assertNotIn(str(self.archive), config)
        env['PATH'] = self.env['PATH']
        def dotfile(*args):
            result = subprocess.run([self.binary, 'dotfile', *args], cwd=self.root, env=env,
                                    capture_output=True, timeout=90)
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors='replace'))
            return result.stdout
        dotfile('init')
        self.assertIn(b'.settings', dotfile('diff'))
        self.assertFalse((self.target / '.settings').exists())
        dotfile('apply')
        dotfile('verify')
        self.assertEqual((self.target / '.settings').read_text(), 'fixture configuration v1\n')
        self.assertEqual((self.target / '.vendor/version').read_text(), 'vendor fixture\n')
        self.assertEqual((self.target / '.Brewfile').read_text(), 'cask "gpg-suite"\n')
        self.assertEqual(dotfile('diff').strip(), b'0 change(s), 0 conflict(s)')
        self.assertFalse((self.target / '.gnupg').exists())
        (repository / 'dot_settings').write_text('fixture configuration v2\n')
        commit(repository)
        dotfile('update')
        dotfile('verify')
        self.assertEqual((self.target / '.settings').read_text(), 'fixture configuration v2\n')
        self.restore('--gpg')
        self.sign_and_decrypt([personal, work])

    def test_default_all_restores_personal_work_subkeys_trust_revocations_and_ssh(self):
        personal = self.generate('Personal <personal@example.invalid>')
        work = self.generate('Work <work@example.invalid>')
        ssh = self.ssh()
        self.backup()
        cipher = self.archive.read_bytes()
        for marker in (personal.encode(), work.encode(), b'BEGIN OPENSSH PRIVATE KEY', b'manifest.json', b'personal@example.invalid'):
            self.assertNotIn(marker, cipher)
        self.restore()
        self.sign_and_decrypt([personal, work])
        self.assertEqual((self.target / '.ssh/github_rsa').read_bytes(), ssh.read_bytes())
        for key in (personal, work):
            name = key + '.rev'
            self.assertEqual((self.target / '.gnupg/openpgp-revocs.d' / name).read_bytes(), (self.source_keyring / 'openpgp-revocs.d' / name).read_bytes())
        self.restore()
        self.sign_and_decrypt([personal, work])

    def test_ssh_only_needs_no_gpg_or_git_and_preserves_an_encrypted_private_key(self):
        path = self.ssh('custom key', password='synthetic-original-key-passphrase')
        isolated_bin = self.root / 'empty PATH'
        isolated_bin.mkdir()
        env = dict(self.env, PATH=str(isolated_bin))
        self.run_secrets('backup', '--ssh', '--output', self.archive, env=env)
        self.run_secrets('restore', '--ssh', '--input', self.archive, '--output', self.target, env=env)
        restored = self.target / '.ssh/custom key'
        self.assertEqual(restored.read_bytes(), path.read_bytes())
        askpass = self.root / 'askpass'
        askpass.write_text("#!/bin/sh\nprintf '%s\\n' synthetic-original-key-passphrase\n")
        askpass.chmod(0o700)
        env = dict(self.env, SSH_ASKPASS=str(askpass), SSH_ASKPASS_REQUIRE='force', DISPLAY='stool-test')
        public = subprocess.run(['ssh-keygen', '-y', '-f', str(restored)], stdin=subprocess.DEVNULL, capture_output=True, env=env, check=True, timeout=30).stdout.split()[:2]
        self.assertEqual(public, path.with_suffix('.pub').read_bytes().split()[:2])
        self.assertFalse((self.target / '.gnupg').exists())

    def test_gpg_original_passphrase_survives_offline_backup_and_restore(self):
        # Only synthetic credentials are embedded here. A private pinentry stub
        # serves the agent protocol without accessing the user's keyring/UI.
        pinentry = self.root / 'pinentry-fixture'
        pinentry.write_text("#!/bin/sh\nprintf 'OK fixture ready\\n'\nwhile IFS= read -r line; do\n  case \"$line\" in\n    GETPIN*) printf 'D synthetic-original-gpg-passphrase\\n' ;;\n    'GETINFO pid'*) printf 'D %s\\n' \"$$\" ;;\n  esac\n  printf 'OK\\n'\n  [ \"$line\" != BYE ] || break\ndone\n")
        pinentry.chmod(0o700)
        config = 'pinentry-program ' + str(pinentry) + '\n'
        (self.source_keyring / 'gpg-agent.conf').write_text(config)
        self.gpg(self.source, '--passphrase', 'synthetic-original-gpg-passphrase',
                 '--quick-generate-key', 'Protected <protected@example.invalid>', 'ed25519', 'sign', '0')
        key = next(iter(groups(self.gpg(self.source, '--with-colons', '--fingerprint', '--list-secret-keys'))))
        self.kill(self.source_keyring)
        try:
            self.backup('--gpg')
        except AssertionError as error:
            # Only this generated fixture is inspected; private export bytes are discarded.
            diagnostic = subprocess.run(['gpg', '--no-options', '--no-tty', '--homedir',
                                         str(self.source_keyring), '--armor', '--export-secret-keys', key],
                                        env=self.env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.PIPE, timeout=30)
            self.fail(str(error) + '\nSynthetic GPG export diagnostic: ' + diagnostic.stderr.decode(errors='replace'))
        (self.target / '.gnupg').mkdir(parents=True, mode=0o700)
        (self.target / '.gnupg/gpg-agent.conf').write_text(config)
        self.restore('--gpg')
        self.kill(self.target / '.gnupg')
        failed = self.gpg(self.target, '--passphrase', 'wrong-original-passphrase',
                          '--local-user', key, '--clearsign', input=b'protected key proof\n', ok=False)
        self.kill(self.target / '.gnupg')
        signature = self.gpg(self.target, '--passphrase', 'synthetic-original-gpg-passphrase',
                             '--local-user', key, '--clearsign', input=b'protected key proof\n')
        self.assertIn(b'VALIDSIG ', self.gpg(self.target, '--status-fd', '1', '--verify', input=signature))

    def test_agent_only_ssh_identity_is_reported_without_exporting_it(self):
        if not shutil.which('ssh-agent') or not shutil.which('ssh-add'):
            self.skipTest('Native SSH agent is unavailable')
        import re
        external = self.root / 'external-agent-key'
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(external)],
                       env=self.env, check=True, capture_output=True)
        startup = subprocess.check_output(['ssh-agent', '-s'], env=self.env).decode()
        sock = re.search(r'SSH_AUTH_SOCK=([^;]+);', startup).group(1)
        pid = re.search(r'SSH_AGENT_PID=([0-9]+);', startup).group(1)
        env = dict(self.env, SSH_AUTH_SOCK=sock, SSH_AGENT_PID=pid)
        self.addCleanup(lambda: subprocess.run(['ssh-agent', '-k'], env=env, capture_output=True))
        subprocess.run(['ssh-add', str(external)], env=env, check=True, capture_output=True)
        self.ssh()
        result = self.run_secrets('backup', '--ssh', '--output', self.archive, env=env)
        self.assertIn(b'has no private-key file under .ssh', result.stderr)
        self.restore('--ssh')
        self.assertTrue((self.target / '.ssh/github_rsa').is_file())
        self.assertFalse((self.target / '.ssh/external-agent-key').exists())

    def test_ciphertext_size_limit_never_publishes_an_unrestorable_backup(self):
        path = self.ssh()
        # This remains a valid OpenSSH file with trailing whitespace. Its tar
        # fits below 64 MiB, but age's authenticated chunk framing does not.
        with path.open('ab') as stream:
            stream.write(b'\n' * (64 * 1024 * 1024 - 8192 - path.stat().st_size))
        result = self.run_secrets('backup', '--ssh', '--output', self.archive, ok=False)
        self.assertIn(b'Encrypted key backup exceeds the 64 MiB size limit', result.stderr)
        self.assertFalse(self.archive.exists())

    def test_gpg_only_restore_from_all_archive_does_not_create_ssh_files(self):
        key = self.generate('Personal <personal@example.invalid>')
        self.ssh()
        self.backup()
        self.restore('--gpg')
        self.sign_and_decrypt([key])
        self.assertFalse((self.target / '.ssh').exists())

    def test_ssh_only_restore_from_all_archive_does_not_need_gpg(self):
        self.generate('Personal <personal@example.invalid>')
        self.ssh()
        self.backup()
        directory = self.root / 'empty PATH'
        directory.mkdir()
        self.run_secrets('restore', '--ssh', '--input', self.archive, '--output', self.target, env=dict(self.env, PATH=str(directory)))
        self.assertFalse((self.target / '.gnupg').exists())
        self.assertTrue((self.target / '.ssh/github_rsa').is_file())

    def test_corrupt_or_wrong_password_backup_never_writes_target(self):
        self.ssh()
        self.backup('--ssh')
        original = self.password.read_bytes()
        self.password.write_text('incorrect password\n')
        self.restore(ok=False)
        self.assertFalse(self.target.exists())
        self.password.write_bytes(original)
        data = bytearray(self.archive.read_bytes())
        data[-1] ^= 1
        self.archive.write_bytes(data)
        self.restore(ok=False)
        self.assertFalse(self.target.exists())

    def test_conflict_is_checked_before_gpg_import_and_private_permissions_are_restored(self):
        self.generate('Personal <personal@example.invalid>')
        self.ssh()
        self.backup()
        (self.target / '.ssh').mkdir(parents=True)
        key = self.target / '.ssh/github_rsa'
        key.write_text('existing key')
        self.restore(ok=False)
        self.assertEqual(key.read_text(), 'existing key')
        self.assertFalse((self.target / '.gnupg').exists())
        key.unlink()
        self.restore()
        self.assertEqual(key.stat().st_mode & 0o777, 0o600)
        self.assertEqual((self.target / '.ssh').stat().st_mode & 0o777, 0o700)
        self.assertEqual((self.target / '.gnupg').stat().st_mode & 0o777, 0o700)


if __name__ == '__main__':
    unittest.main()
