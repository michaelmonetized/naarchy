#!/usr/bin/env bash
# Package already-validated local binaries and an exact committed source tree.
# Does not build remotely, install, push, or publish.
set -euo pipefail
if [[ "$#" != 4 ]]; then
  echo "Usage: $0 ARM64_BINARY X86_64_BINARY OUTPUT_DIRECTORY COMMIT" >&2
  exit 2
fi
repo="$(cd -- "$(dirname -- "$0")/.." && pwd)"
python3 - "$repo" "$@" <<'PY'
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

repo = pathlib.Path(sys.argv[1])
arm, x86 = (pathlib.Path(p).resolve(strict=True) for p in sys.argv[2:4])
out = pathlib.Path(sys.argv[4]).resolve()
ref = sys.argv[5]
def git(*args):
    return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()
commit = git('rev-parse', '--verify', ref + '^{commit}')
tree = git('rev-parse', commit + '^{tree}')
version = tomllib.loads(git('show', commit + ':Cargo.toml'))['package']['version']
if not re.fullmatch(r'\d+\.\d+\.\d+', version):
    raise SystemExit('Release version must be numeric MAJOR.MINOR.PATCH')
if git('status', '--porcelain', '--untracked-files=no'):
    raise SystemExit('Commit tracked changes before packaging')
out.mkdir(parents=True, exist_ok=True)
names = []
with tempfile.TemporaryDirectory(prefix='naarchy-package-') as tmp:
    tmp = pathlib.Path(tmp)
    source = out / f'naarchy-{version}-src.tar.gz'
    with source.open('wb') as dest:
        subprocess.run(['git', 'archive', '--format=tar.gz',
                        f'--prefix=naarchy-{version}/', commit],
                       cwd=repo, stdout=dest, check=True)
    with tarfile.open(source) as archive:
        archive.extractall(tmp, filter='data')
    source_root = tmp / f'naarchy-{version}'
    names.append(source.name)
    for target, binary, machine in [
        ('aarch64-unknown-linux-gnu', arm, 'AArch64'),
        ('x86_64-unknown-linux-gnu', x86, 'Advanced Micro Devices X86-64'),
    ]:
        header = subprocess.check_output(['readelf', '-h', str(binary)], text=True)
        if machine not in header:
            raise SystemExit(f'Wrong ELF architecture for {target}')
        versions = subprocess.check_output(['readelf', '--version-info', str(binary)], text=True)
        glibc = [tuple(map(int, v.split('.'))) for v in re.findall(r'GLIBC_(\d+\.\d+)', versions)]
        if glibc and max(glibc) > (2, 39):
            raise SystemExit(f'{target} requires glibc newer than the documented 2.39')
        package = tmp / f'naarchy-{target}'
        (package / 'bin').mkdir(parents=True)
        shutil.copy2(binary, package / 'bin/naarchy')
        for folder in ('contrib', 'scripts'):
            (package / folder).mkdir()
        for name in ('naarchy.desktop', 'naarchy.service', 'app.naarchy.Naarchy.svg', 'hyprland.conf'):
            shutil.copy2(source_root / 'contrib' / name, package / 'contrib' / name)
        shutil.copy2(source_root / 'scripts/install.sh', package / 'scripts/install.sh')
        for name in ('README.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'LICENSE'):
            shutil.copy2(source_root / name, package / name)
        shutil.copytree(source_root / 'docs', package / 'docs')
        (package / 'BUILDINFO.json').write_text(json.dumps({
            'version': version, 'commit': commit, 'source_tree': tree,
            'target': target, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        }, indent=2) + '\n')
        archive_path = out / f'{package.name}.tar.gz'
        with tarfile.open(archive_path, 'w:gz') as archive:
            archive.add(package, arcname=package.name)
        names.append(archive_path.name)
    checksums = ''.join(hashlib.sha256((out / name).read_bytes()).hexdigest()
                        + '  ' + name + '\n' for name in sorted(names))
    (out / 'SHA256SUMS').write_text(checksums)
    print(f'Packaged Naarchy {version} from {commit}')
    print(checksums, end='')
PY
