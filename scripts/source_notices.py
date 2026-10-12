"""Read notices from native source archives without executing build recipes."""
import io
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile

from native_evidence import digest


def is_notice(name):
    path = PurePosixPath(name)
    return (any(part.lower() in {'licenses', 'licences'} for part in path.parts[:-1])
            or bool(re.match(r'^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)([._-]|$)',
                             path.name, re.I)))


def source_notices(archive, destination):
    """Read notices in tar payloads and pinned bare Git sources without running recipes."""
    destination.mkdir(parents=True, exist_ok=True)
    records = []

    def save(name, stream):
        relative = PurePosixPath(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError(f'Unsafe source notice: {name}')
        path = destination.joinpath(*relative.parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as output:
            shutil.copyfileobj(stream, output)
        records.append({'path': relative.as_posix(), 'sha256': digest(path)})

    def scan(path, prefix='', depth=0):
        if depth > 3:
            raise ValueError(f'Source nesting requires review: {path}')
        with tarfile.open(path) as source, tempfile.TemporaryDirectory() as temporary:
            members = source.getmembers()
            names = {member.name for member in members}
            for member in members:
                if not member.isfile():
                    continue
                if is_notice(member.name):
                    with source.extractfile(member) as stream:
                        save(prefix + member.name, stream)
                elif re.search(r'\.(tar\.(gz|xz|bz2|zst)|tgz)$', member.name):
                    nested = Path(temporary) / 'nested.tar'
                    with source.extractfile(member) as stream, nested.open('wb') as output:
                        shutil.copyfileobj(stream, output)
                    scan(nested, prefix + member.name + '.contents/', depth + 1)
            # makepkg source archives can store a bare repository instead of a
            # tarball. Only the commit declared in .SRCINFO supplies notices.
            info = next((m for m in members if m.name.endswith('/.SRCINFO')), None)
            if info is None:
                return
            description = source.extractfile(info).read().decode()
            for line in description.splitlines():
                if ' = ' not in line or not line.strip().startswith('source'):
                    continue
                value = line.split(' = ', 1)[1]
                match = re.fullmatch(r'(?:([^:]+)::)?git\+([^#]+)#commit=([0-9a-f]{40})', value)
                if not match:
                    continue
                alias, url, commit = match.groups()
                folder = alias or url.rstrip('/').rsplit('/', 1)[1].removesuffix('.git')
                repo_prefix = str(PurePosixPath(info.name).parent / folder) + '/'
                if repo_prefix + 'HEAD' not in names:
                    raise ValueError(f'Pinned Git source missing from archive: {folder}')
                git_root = Path(temporary) / 'git-source'
                git_root.mkdir(exist_ok=True)
                source.extractall(git_root, members=[m for m in members if m.name.startswith(repo_prefix)], filter='data')
                repository = git_root / repo_prefix
                command = ['git', '--no-replace-objects', '--git-dir', str(repository)]
                files = subprocess.check_output(command + ['ls-tree', '-r', '--name-only', '-z', commit]).split(b'\0')
                for file in files:
                    name = file.decode()
                    if name and is_notice(name):
                        content = subprocess.check_output(command + ['show', f'{commit}:{name}'])
                        save(prefix + repo_prefix + name, io.BytesIO(content))
    scan(archive)
    return records
