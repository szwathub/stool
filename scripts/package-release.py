#!/usr/bin/env python3
"""Package a built stool binary using the installer's version/target convention."""
import argparse
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import zipfile

TARGETS = (
    'x86_64-unknown-linux-gnu',
    'x86_64-unknown-linux-musl',
    'aarch64-unknown-linux-gnu',
    'x86_64-apple-darwin',
    'aarch64-apple-darwin',
    'x86_64-pc-windows-msvc',
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True, choices=TARGETS)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--output', type=Path, default=Path('artifacts'))
    args = parser.parse_args()
    if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?', args.tag):
        parser.error('--tag must be vMAJOR.MINOR.PATCH, optionally with a prerelease suffix')
    root = Path(__file__).resolve().parents[1]
    binary_name = 'stool.exe' if args.target.endswith('-windows-msvc') else 'stool'
    binary = args.binary or root / 'target' / args.target / 'release' / binary_name
    if not binary.is_file():
        parser.error(f'binary not found: {binary}')
    args.output.mkdir(parents=True, exist_ok=True)
    basename = f'stool-{args.tag}-{args.target}'
    with tempfile.TemporaryDirectory(prefix='stool-package-') as temporary:
        package = Path(temporary) / basename
        package.mkdir()
        shutil.copy2(binary, package / binary_name)
        (package / binary_name).chmod(0o755)
        for filename in ('README.md', 'RELEASE_NOTES.md', 'LICENSE'):
            shutil.copy2(root / filename, package / filename)
        for directory in ('completions', 'man-page'):
            shutil.copytree(root / directory, package / directory,
                            ignore=shutil.ignore_patterns('.DS_Store', '__pycache__', '*.pyc'))
        if args.target.endswith('-windows-msvc'):
            archive = args.output / f'{basename}.zip'
            with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as output:
                for path in sorted(package.rglob('*')):
                    if path.is_file():
                        output.write(path, path.relative_to(package.parent))
        else:
            archive = args.output / f'{basename}.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                output.add(package, arcname=basename)
    print(archive)


if __name__ == '__main__':
    main()
