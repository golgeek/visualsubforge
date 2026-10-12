"""Give installed Windows notices short paths without losing provenance or bytes.

NSIS 3.11 cannot open some expanded dependency-source paths beyond MAX_PATH.
Only the new, assembled package is rewritten; source archives keep their names.
The index maps every original notice path to a content-addressed installed file.
"""
import json
from pathlib import Path
import shutil
import subprocess

from native_evidence import digest


def compact(bundle):
    original = bundle / 'third-party'
    if (original / 'index.json').exists():
        raise ValueError('Notice paths have already been compacted')
    staged = bundle / 'third-party-staging'
    staged.mkdir(exist_ok=False)
    (staged / 'files').mkdir()
    records = []
    for source in sorted(original.rglob('*')):
        if source.is_symlink():
            raise ValueError(f'Unexpected notice symlink: {source}')
        if not source.is_file():
            continue
        checksum = digest(source)
        name = checksum + '.txt'
        target = staged / 'files' / name
        if not target.exists():
            shutil.copy2(source, target)
        if digest(target) != checksum:
            raise ValueError(f'Notice bytes changed while staging: {source}')
        records.append({'source_path': source.relative_to(bundle).as_posix(),
                        'path': 'third-party/files/' + name, 'sha256': checksum})
    if not records:
        raise ValueError('No third-party notices were assembled')
    index = {'schema': 1, 'scope': __doc__.strip(), 'files': records}
    (staged / 'index.json').write_text(json.dumps(index, indent=2) + '\n', encoding='utf-8')
    (staged / 'README.txt').write_text(
        'Third-party license notices\n\n'
        'index.json maps each original source/notice path to a file here.\n'
        'Identical notices share a file; every notice is preserved byte-for-byte.\n'
        'The .txt suffix does not change the original encoding or contents.\n'
        'Matching application/native source archives retain their original paths.\n', encoding='utf-8')
    # This is a disposable assembly directory, not a user installation. Finish
    # copying and checking all notice bytes before replacing its expanded tree.
    shutil.rmtree(original)
    staged.rename(original)
    return index


def check_nsis(output, executable, log):
    """Exercise the real pinned compiler and silent install before native builds."""
    root = output / 'notice-control'
    root.mkdir()
    bundle = root / 'bundle'
    parent = bundle / 'third-party' / 'native'
    while len(str(parent / 'LICENSE.txt')) <= 280:
        parent /= 'nested-source-notices'
    parent.mkdir(parents=True)
    source = parent / 'LICENSE.txt'
    source.write_bytes(b'Preserve this notice exactly.\r\n\xff\n')
    checksum = digest(source)
    commands = []

    def compile_installer(name, files):
        installer = root / (name + '.exe')
        script = root / (name + '.nsi')
        lines = ['Unicode true', 'Name "VSF notice control"',
                 f'OutFile "{installer}"', 'RequestExecutionLevel user', 'Section']
        for path, relative in files:
            destination = str(relative.parent).replace('/', '\\')
            lines += [f'SetOutPath "$INSTDIR\\{destination}"',
                      f'File "/oname={relative.name}" "{path}"']
        script.write_text('\n'.join(lines + ['SectionEnd', '']), encoding='utf-8')
        command = [str(executable), '/V2', '/NOCD', str(script)]
        commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        diagnostic = result.stdout + result.stderr
        log.write(json.dumps(command) + '\n' + diagnostic)
        log.flush()
        return installer, result.returncode, diagnostic

    _, original_exit, diagnostic = compile_installer('original', [(source, Path('LICENSE.txt'))])
    if original_exit == 0 or 'failed opening file' not in diagnostic:
        raise ValueError('Pinned NSIS did not reproduce the expanded notice-path failure')
    index = compact(bundle)
    files = [(p, p.relative_to(bundle)) for p in sorted(bundle.rglob('*')) if p.is_file()]
    installer, exit_code, _ = compile_installer('compact', files)
    if exit_code:
        raise ValueError(f'NSIS could not compile compact notices: {exit_code}')
    installed = root / 'Installed Notice Control'
    # NSIS /D is last and unquoted, even for an installation path with spaces.
    command = f'"{installer}" /S /D={installed}'
    commands.append(command)
    subprocess.run(command, check=True, timeout=60, stdout=log, stderr=subprocess.STDOUT)
    for path, relative in files:
        if digest(installed / relative) != digest(path):
            raise ValueError(f'NSIS changed or omitted an installed notice: {relative}')
    if digest(installed / index['files'][0]['path']) != checksum:
        raise ValueError('Installed notice differs from the original long-path file')
    return {'status': 'passed', 'commands': commands, 'original_exit': original_exit,
            'original_path_length': len(str(source)), 'notice_sha256': checksum,
            'installed_files_verified': len(files)}
