"""Preserve sources and notices selected by the actual bundled Homebrew kegs.

Installed formulae, receipts and SBOMs are evidence. Today's formula cannot
replace an older build recipe; missing local patches require an exact historical
recipe match (apart from the bottle metadata Homebrew omits from installed copies).
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.parse
import urllib.request

from native_evidence import digest
from source_archive import archive_source
from source_notices import is_notice, source_notices

ROOT = Path(__file__).resolve().parent.parent


def without_bottle_metadata(text):
    return re.sub(r'\n  bottle do\n.*?^  end\n', '', text, flags=re.M | re.S)


def fetch(url, path, checksum=None):
    if path.exists():
        raise ValueError(f'Refusing to replace source evidence: {path}')
    with urllib.request.urlopen(url, timeout=120) as response, path.open('xb') as stream:
        shutil.copyfileobj(response, stream)
    actual = digest(path)
    if checksum and actual != checksum:
        raise ValueError(f'Homebrew source checksum mismatch: {url}')
    return {'url': url, 'file': path.name, 'sha256': actual}


def matching_recipe(formula, directory):
    """Search only public Homebrew core history; never use a moving patch URL."""
    name = formula.stem
    original = formula.read_text()
    relative = f'Formula/{name[0]}/{name}.rb'
    candidates = []
    for page in [1, 2]:
        api = 'repos/Homebrew/homebrew-core/commits?' + urllib.parse.urlencode({
            'path': relative, 'per_page': 100, 'page': page})
        commits = json.loads(subprocess.check_output(['gh', 'api', api], text=True))
        for commit in commits:
            sha = commit['sha']
            url = f'https://raw.githubusercontent.com/Homebrew/homebrew-core/{sha}/{relative}'
            with urllib.request.urlopen(url, timeout=60) as response:
                contents = response.read()
            candidates.append(sha)
            if without_bottle_metadata(contents.decode()) == without_bottle_metadata(original):
                path = directory / 'upstream-formula.rb'
                path.write_bytes(contents)
                return {'commit': sha, 'path': relative, 'url': url, 'sha256': digest(path),
                        'comparison': 'Exact recipe text apart from bottle metadata'}
        if len(commits) < 100:
            break
    raise ValueError(f'No matching historical formula for {name} in {len(candidates)} revisions')


def capture_resource(resource, destination):
    destination.mkdir(parents=True, exist_ok=False)
    url, checksum = resource['url'], resource['checksum']
    if checksum:
        if not re.fullmatch(r'[0-9a-f]{64}', checksum):
            raise ValueError(f'Unreviewed source checksum: {resource}')
        filename = Path(urllib.parse.urlparse(url).path).name
        archive = destination / filename
        record = fetch(url, archive, checksum)
    else:
        commit = resource.get('specs', {}).get('revision')
        if not commit or not re.fullmatch(r'[0-9a-f]{40}', commit):
            raise ValueError(f'Source has no checksum or exact Git revision: {resource}')
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary) / 'source'
            subprocess.run(['git', 'init', '-q', checkout], check=True)
            subprocess.run(['git', '-C', checkout, 'remote', 'add', 'origin', url], check=True)
            subprocess.run(['git', '-C', checkout, 'fetch', '--depth=1', 'origin', commit], check=True)
            subprocess.run(['git', '-C', checkout, 'checkout', '--detach', 'FETCH_HEAD'], check=True)
            actual = subprocess.check_output(['git', '-C', checkout, 'rev-parse', 'HEAD'], text=True).strip()
            if actual != commit:
                raise ValueError(f'Source checkout mismatch: {actual} != {commit}')
            subprocess.run(['git', '-C', checkout, 'submodule', 'update', '--init', '--recursive', '--depth=1'], check=True)
            archive = destination / 'source.tar.gz'
            archive_source(checkout, archive)
        record = {'url': url, 'commit': commit, 'file': archive.name, 'sha256': digest(archive)}
    record['notices'] = source_notices(archive, destination / 'notices')
    return record


def verify_source_identity(formula, sbom):
    """Require the installed bottle's source identity to agree with its recipe."""
    sources = [package for package in sbom['packages']
               if package['SPDXID'] == f'SPDXRef-Archive-{formula["name"]}-src']
    if len(sources) != 1:
        raise ValueError(f'Missing or ambiguous Homebrew source identity: {formula["name"]}')
    source = sources[0]
    expected = formula['stable']
    checksums = [item['checksumValue'] for item in source['checksums'] if item['algorithm'] == 'SHA256']
    if source['downloadLocation'] != expected['url'] or checksums != [expected['checksum']]:
        raise ValueError(f'Installed SBOM and formula source differ: {formula["name"]}')


def collect(records, cellar, output):
    cellar = cellar.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    groups = {}
    for item in records:
        source = Path(item['source']).resolve(strict=True)
        if not source.is_relative_to(cellar):
            continue  # VSF/mpv/ANGLE sources have their own captures.
        if digest(source) != item['source_sha256']:
            raise ValueError(f'Native input changed before inventory: {source}')
        relative = source.relative_to(cellar)
        name, version = relative.parts[:2]
        group = groups.setdefault((name, version), {'keg': cellar / name / version, 'files': []})
        group['files'].append({'bundled_path': item['file'], 'keg_path': Path(*relative.parts[2:]).as_posix(),
                               'source_sha256': item['source_sha256']})
    if not groups:
        raise ValueError('No Homebrew native inputs found in the package')
    formula_paths = [group['keg'] / '.brew' / (name + '.rb') for (name, _), group in groups.items()]
    env = dict(os.environ, HOMEBREW_NO_AUTO_UPDATE='1', HOMEBREW_DEVELOPER='1')
    formulae = json.loads(subprocess.check_output(['brew', 'ruby', ROOT / 'scripts/homebrew-sources.rb',
                                                  *formula_paths], env=env, text=True))
    inventory = {'schema': 1, 'packages': [], 'scope': 'Installed Homebrew inputs; custom and OS runtimes are separate'}
    try:
        for ((name, version), group), formula in zip(groups.items(), formulae, strict=True):
            if formula['name'] != name or formula['version'] != version:
                raise ValueError(f'Installed formula does not identify the bundled keg: {name} {version}')
            print(f'Capture Homebrew source: {name} {version}', flush=True)
            directory = output / f'{name}-{version}'
            directory.mkdir()
            keg = group['keg']
            original = keg / '.brew' / (name + '.rb')
            shutil.copy2(original, directory / 'installed-formula.rb')
            for filename in ['INSTALL_RECEIPT.json', 'sbom.spdx.json']:
                shutil.copy2(keg / filename, directory / filename)
            verify_source_identity(formula, json.loads((keg / 'sbom.spdx.json').read_text()))
            record = {'name': name, 'version': version, 'license': formula['license'], 'files': group['files'],
                      'formula_sha256': digest(original), 'source': capture_resource(formula['stable'], directory / 'source'),
                      'resources': [], 'patches': []}
            # Retain ancillary sources even when used only during the build.
            for index, resource in enumerate(formula['resources']):
                record['resources'].append({'name': resource['name'],
                    **capture_resource(resource, directory / f'resource-{index}')})
            revision = None
            for index, patch in enumerate(formula['patches']):
                patch_path = directory / f'patch-{index}.diff'
                captured = {key: value for key, value in patch.items() if key != 'contents'}
                if 'contents' in patch:
                    patch_path.write_text(patch['contents'])
                elif 'file' in patch:
                    relative = Path(patch['file'])
                    if relative.is_absolute() or '..' in relative.parts:
                        raise ValueError(f'Unsafe local formula patch: {relative}')
                    if revision is None:
                        revision = matching_recipe(original, directory)
                    url = f'https://raw.githubusercontent.com/Homebrew/homebrew-core/{revision["commit"]}/{relative.as_posix()}'
                    captured.update(fetch(url, patch_path))
                else:
                    if not re.fullmatch(r'[0-9a-f]{64}', patch['checksum']):
                        raise ValueError(f'External patch has no checksum: {patch}')
                    captured.update(fetch(patch['url'], patch_path, patch['checksum']))
                captured.update(file=patch_path.name, sha256=digest(patch_path))
                record['patches'].append(captured)
            record['historical_recipe'] = revision
            # Top-level or packaged notices supplement those read from sources.
            notices = directory / 'installed-notices'
            notice_files = [path for path in keg.rglob('*') if path.is_file() and not path.is_symlink()
                            and is_notice(path.relative_to(keg).as_posix())]
            for path in notice_files:
                target = notices / path.relative_to(keg)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
            if not record['source']['notices'] and not notice_files:
                raise ValueError(f'No license notices found for {name}')
            record['evidence_files_sha256'] = {p.relative_to(directory).as_posix(): digest(p)
                                               for p in sorted(directory.rglob('*')) if p.is_file()}
            inventory['packages'].append(record)
        inventory['status'] = 'captured'
    finally:
        (output / 'homebrew-inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(path)}  {path.relative_to(output).as_posix()}\n'
        for path in sorted(output.rglob('*')) if path.is_file()))
    return inventory
