"""Bind the Mac bundle's native inputs to Homebrew and custom-build sources.

Called before signing: notices and the inventory become sealed app resources.
Source archives stay alongside the download, outside the installed application.
"""
import json
from pathlib import Path
import shutil

from homebrew_sources import collect as collect_homebrew
from native_evidence import digest
from source_notices import source_notices


def check_custom_file(path, checksum, receipt):
    entries = [entry for entry in receipt['binaries']
               if Path(entry.get('path', entry.get('file', ''))).name == path.name]
    if len(entries) != 1 or entries[0]['sha256'] != checksum:
        raise ValueError(f'Custom binary differs from its build receipt: {path}')


def collect(records, cellar, mpv, angle_sdk, angle_receipt, output, resources):
    cellar, mpv, angle_sdk = (path.resolve(strict=True) for path in [cellar, mpv, angle_sdk])
    output.mkdir(parents=True, exist_ok=False)
    receipts = {'mpv': json.loads((mpv / 'build-receipt.json').read_text()),
                'angle': json.loads(angle_receipt.read_text())}
    if receipts['mpv']['status'] != 'passed' or receipts['angle']['status'] != 'built':
        raise ValueError('Native sources require successful mpv and ANGLE build receipts')
    bindings = []
    for item in records:
        original = Path(item['source']).resolve(strict=True)
        if digest(original) != item['source_sha256']:
            raise ValueError(f'Native input changed before source capture: {original}')
        if original.is_relative_to(cellar):
            origin = 'homebrew'
        elif original.is_relative_to(mpv / 'build'):
            origin = 'mpv'
        elif original.is_relative_to(angle_sdk / 'out/Metal'):
            origin = 'angle'
        elif item['file'] in ['Contents/MacOS/vsf', 'Contents/MacOS/visualsubforge']:
            origin = 'application-inventory.json'
        else:
            raise ValueError(f'Unaccounted native file origin: {original}')
        if origin in receipts:
            check_custom_file(original, item['source_sha256'], receipts[origin])
        bindings.append({'path': item['file'], 'source_sha256': item['source_sha256'], 'origin': origin})
    homebrew = collect_homebrew(records, cellar, output / 'homebrew')
    custom = output / 'custom'
    custom.mkdir()
    inventory = {'schema': 1, 'files': bindings, 'homebrew': homebrew, 'custom': {}}
    for name, archive, receipt_file in [('mpv', mpv / 'source.tar.gz', mpv / 'build-receipt.json'),
                                         ('angle', angle_sdk / 'source.tar.gz', angle_receipt)]:
        if digest(archive) != receipts[name]['source_archive_sha256']:
            raise ValueError(f'Custom source differs from its build receipt: {name}')
        shutil.copy2(archive, custom / f'{name}-source.tar.gz')
        shutil.copy2(receipt_file, custom / f'{name}-build.json')
        notices = source_notices(archive, output / 'notices' / name)
        if not notices:
            raise ValueError(f'No custom source notices: {name}')
        inventory['custom'][name] = {'source_sha256': digest(archive), 'notices': notices,
                                    'build_receipt_sha256': digest(receipt_file)}
    shutil.copy2(angle_sdk / 'source.tar.gz.json', custom / 'angle-source.json')
    shutil.copy2(angle_sdk / 'dependency-revisions.txt', custom / 'angle-dependency-revisions.txt')
    # Bundle only notices, not all corresponding source payloads.
    for package in homebrew['packages']:
        relative = Path(f'{package["name"]}-{package["version"]}')
        for filename in package['evidence_files_sha256']:
            path = Path(filename)
            if 'notices' in path.parts or 'installed-notices' in path.parts:
                target = output / 'notices/homebrew' / relative / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(output / 'homebrew' / relative / path, target)
    inventory['scope'] = ('Bundled Homebrew and custom native inputs, their sources/recipes and notices; '
                          'OS runtimes and static/header-only contributions require separate review')
    inventory['evidence_files_sha256'] = {p.relative_to(output).as_posix(): digest(p)
                                         for p in sorted(output.rglob('*')) if p.is_file()}
    path = output / 'native-inventory.json'
    path.write_text(json.dumps(inventory, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.relative_to(output).as_posix()}\n'
        for p in sorted(output.rglob('*')) if p.is_file()))
    shutil.copy2(path, resources / 'native-inventory.json')
    shutil.copytree(output / 'notices', resources / 'third-party/native')
    return digest(path)
