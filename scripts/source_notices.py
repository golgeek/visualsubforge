"""Read notices from native source archives without executing build recipes."""
import io
from contextlib import contextmanager
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile

from native_evidence import digest

# Rust 1.97 vendors these Insta tests. Their assert_binary_snapshot! stores the
# literal bytes b"test" with a multipart extension; these are not tar archives.
# Keep the version/path and bytes explicit so an unknown or changed payload
# still fails archive verification. The complete source archive retains them.
INSTA_EXTENSION_FIXTURES = tuple(
    f'/vendor/insta-{version}/tests/snapshots/test_binary__multipart_extension.snap.tar.gz'
    for version in ('1.43.0', '1.47.2')
)


def is_notice(name):
    path = PurePosixPath(name)
    return (any(part.lower() in {'licenses', 'licences'} for part in path.parts[:-1])
            or bool(re.match(r'^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)([._-]|$)',
                             path.name, re.I)))


@contextmanager
def source_tar(archive, lzip):
    # Python's tarfile handles the other source formats directly, but not
    # lzip (used by GNU gettext). Decode to a temporary file so lzip verifies
    # its checksum before any notices are accepted; never extract recipes.
    with tempfile.TemporaryDirectory() as temporary:
        path = archive
        if archive.suffix == '.lz':
            path = Path(temporary) / 'source.tar'
            with path.open('wb') as output:
                result = subprocess.run([lzip, '--decompress', '--stdout', str(archive)],
                                        stdout=output, stderr=subprocess.PIPE)
            if result.returncode:
                raise ValueError(f'Cannot decode lzip source {archive}: '
                                 + result.stderr.decode(errors='replace'))
        with tarfile.open(path) as source:
            yield source


def source_notices(archive, destination, *, lzip='lzip', non_archive_fixtures=None):
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
        with source_tar(path, lzip) as source, tempfile.TemporaryDirectory() as temporary:
            members = source.getmembers()
            names = {member.name for member in members}
            for member in members:
                if not member.isfile():
                    continue
                if is_notice(member.name):
                    with source.extractfile(member) as stream:
                        save(prefix + member.name, stream)
                elif re.search(r'\.(tar\.(gz|xz|bz2|zst|lz)|tgz)$', member.name):
                    nested = Path(temporary) / ('nested' + ''.join(PurePosixPath(member.name).suffixes))
                    with source.extractfile(member) as stream, nested.open('wb') as output:
                        shutil.copyfileobj(stream, output)
                    name = prefix + member.name
                    if member.name.endswith(INSTA_EXTENSION_FIXTURES):
                        if member.size != 4 or nested.read_bytes() != b'test':
                            raise ValueError(f'Reviewed non-archive fixture changed: {name}')
                        if non_archive_fixtures is not None:
                            non_archive_fixtures.append({'path': name, 'sha256': digest(nested),
                                'reason': 'Insta multipart-extension snapshot containing literal test bytes'})
                        continue
                    try:
                        scan(nested, name + '.contents/', depth + 1)
                    except tarfile.ReadError as error:
                        raise ValueError(f'Cannot read nested source archive {name}: {error}') from error
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
