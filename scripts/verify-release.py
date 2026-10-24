#!/usr/bin/env python3
"""Verify a published Linux install and staged key/dotfile bootstrap with synthetic identities."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--binary', type=Path, help='Prepublication check: use this built binary instead of downloading')
    args = parser.parse_args()
    if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+', args.tag):
        parser.error('--tag must be a stable vMAJOR.MINOR.PATCH version')
    if sys.platform != 'linux':
        parser.error('Full bootstrap verification uses Linux; macOS Apple stages are tested with injected executors')
    for name in ('git', 'gpg', 'ssh-keygen'):
        if not shutil.which(name):
            parser.error(f'Install {name} to generate and verify disposable fixtures')
    root = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix='stool-release-verify-') as temporary:
        temporary = Path(temporary)
        home = temporary / 'home'
        home.mkdir(mode=0o700)
        env = dict(os.environ, HOME=str(home), USERPROFILE=str(home))
        for name in ('PREFIX', 'VERSION', 'GPG_AGENT_INFO', 'GPG_TTY', 'SSH_AUTH_SOCK', 'SSH_AGENT_PID', 'STOOL_AGE_PASSPHRASE'):
            env.pop(name, None)
        if args.binary:
            binary = args.binary.resolve()
        else:
            installer = temporary / 'install.sh'
            url = f'https://raw.githubusercontent.com/szwathub/stool/{args.tag}/install.sh'
            with urllib.request.urlopen(url, timeout=60) as response:
                installer.write_bytes(response.read())
            blocked = temporary / 'blocked'
            blocked.mkdir()
            log = temporary / 'forbidden-tools'
            for name in ('cargo', 'rustc', 'git', 'brew'):
                command = blocked / name
                command.write_text('#!/bin/sh\nprintf called >> "$STOOL_VERIFY_FORBIDDEN"\nexit 97\n')
                command.chmod(0o700)
            install_env = dict(env, VERSION=args.tag, PREFIX=str(temporary / 'prefix'),
                               PATH=str(blocked) + os.pathsep + env['PATH'], STOOL_VERIFY_FORBIDDEN=str(log))
            subprocess.run(['/bin/bash', str(installer)], env=install_env, check=True, timeout=300)
            if log.exists():
                raise RuntimeError('Installer invoked Git, Homebrew or a compiler')
            binary = temporary / 'prefix/bin/stool'
        version = subprocess.check_output([str(binary), '--version'], env=env, text=True).strip()
        if version != 'stool ' + args.tag[1:]:
            raise RuntimeError(f'Unexpected binary version: {version}')
        os.environ['STOOL_BIN'] = str(binary)
        sys.path.insert(0, str(root / 'tests/python'))
        import test_secrets_native
        suite = unittest.TestSuite([test_secrets_native.NativeSecretsTests(
            'test_init_without_gpg_restores_ssh_then_standalone_restore_recovers_personal_and_work')])
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        if not result.wasSuccessful() or result.skipped or result.testsRun != 1:
            raise RuntimeError('Full bootstrap verification did not pass')
        print(json.dumps({'version': version, 'published_installer': not bool(args.binary),
                          'default_url': True, 'recursive_submodules': True,
                          'dotfiles_diff_apply_verify_update': True,
                          'ssh_before_gpg': True, 'personal_work_sign_decrypt': True}))


if __name__ == '__main__':
    main()
