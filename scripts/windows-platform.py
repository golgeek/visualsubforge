#!/usr/bin/env python3
"""Record actual MSVC/Windows SDK link inputs without redistributing SDK libraries.

The compiler's response-file diagnostic identifies the inputs to the two shipped
executables, including static runtimes absent from the DLL graph. Proprietary
SDK/library payloads are hashed, never copied into a source distribution.
"""
import argparse
import json
import os
from pathlib import Path, PureWindowsPath
import shutil
import subprocess
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

from native_evidence import digest


MICROSOFT_TERMS = [
    ('Visual-Studio-2022-Enterprise-Professional-License-EN.docx',
     'https://visualstudio.microsoft.com/wp-content/uploads/2021/11/',
     '9c0cd52b20db9d081854c75bd1b50c75514b8f8cb09c8cad15e89d90b97b5bf3',
     'Developer distribution terms; reference evidence, not the application license'),
    ('Visual-C-Runtime-2015-2022-License-1.docx',
     'https://visualstudio.microsoft.com/wp-content/uploads/2021/09/',
     'f1e3d56ceb2ad68aae0711b910375009e651ac5530fa0760f0dea6e81e54fae1',
     'Microsoft C runtime end-user terms; separate from original VSF code'),
]


def capture_terms(output):
    """Preserve reviewed publisher documents and readable text, with both hashes."""
    directory = output / 'publisher-terms'
    directory.mkdir()
    records = []
    for filename, base, checksum, scope in MICROSOFT_TERMS:
        url = base + filename
        request = urllib.request.Request(url, headers={
            'User-Agent': 'Mozilla/5.0',
            'Referer': 'https://visualstudio.microsoft.com/license-terms/vs2022-ga-proenterprise/'})
        document = directory / filename
        with urllib.request.urlopen(request, timeout=90) as response, document.open('xb') as target:
            shutil.copyfileobj(response, target)
        if digest(document) != checksum:
            raise ValueError(f'Microsoft terms changed; review the new document: {url}')
        with zipfile.ZipFile(document) as archive:
            root = ET.fromstring(archive.read('word/document.xml'))
        paragraphs = root.findall('.//{http://schemas.openxmlformats.org/wordprocessingml/2006/main}p')
        text = '\n'.join(''.join(paragraph.itertext()) for paragraph in paragraphs) + '\n'
        readable = document.with_suffix('.txt')
        readable.write_text(text, encoding='utf-8')
        records.append({'url': url, 'file': document.relative_to(output).as_posix(),
                        'sha256': checksum, 'scope': scope,
                        'text_file': readable.relative_to(output).as_posix(), 'text_sha256': digest(readable)})
    return records


def input_paths(text):
    paths = []
    for line in text.splitlines():
        line = line.strip().strip('"')
        if not line:
            continue
        # /LINKREPROFULLPATHRSP retains these options before their full paths.
        # Reject unknown options instead of silently losing a linker input.
        for option in ['/defaultlib:', '/natvis:']:
            if line.lower().startswith(option):
                line = line[len(option):].strip('"')
                break
        path = PureWindowsPath(line)
        if not path.is_absolute():
            raise ValueError(f'Invalid MSVC linker input: {line}')
        paths.append(path)
    if not paths:
        raise ValueError('Empty MSVC linker input record')
    return sorted(set(paths))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['inputs', 'binaries', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    compiler = Path(os.environ['VCToolsInstallDir']).resolve(strict=True)
    sdk = Path(os.environ['WindowsSdkDir']).resolve(strict=True)
    receipt = {'schema': 1, 'scope': __doc__.strip(), 'toolchain': {
        name: os.environ[name] for name in ['VCToolsVersion', 'WindowsSDKVersion', 'VCToolsInstallDir', 'WindowsSdkDir']},
        'executables': [], 'license_documents': []}
    for name in ['visualsubforge', 'vsf']:
        response = args.inputs / (name + '.rsp')
        shutil.copy2(response, args.output / response.name)
        records = []
        contents = response.read_bytes()
        encoding = 'utf-16' if contents.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig'
        for input_path in input_paths(contents.decode(encoding)):
            path = Path(input_path).resolve()
            origin = ('MSVC' if path.is_relative_to(compiler) else
                      'Windows SDK' if path.is_relative_to(sdk) else 'Application/Rust/native build')
            # Rust removes temporary object/import files after linking. Their
            # paths remain evidence; compiler/SDK inputs must still be hashable.
            checksum = digest(path) if path.is_file() else None
            if checksum is None and origin != 'Application/Rust/native build':
                raise ValueError(f'Compiler/SDK linker input is unavailable: {path}')
            records.append({'path': str(path), 'sha256': checksum, 'origin': origin})
        if not any(item['origin'] == 'MSVC' for item in records):
            raise ValueError(f'MSVC runtime inputs missing from linker evidence: {name}')
        receipt['executables'].append({'file': name + '.exe',
            'sha256': digest(args.binaries / (name + '.exe')), 'link_inputs': records,
            'response_sha256': digest(response)})
    # Retain installed license/notice texts when supplied by the build tools.
    # Locations differ by SDK/toolset; the inventory records the actual paths.
    for root in [compiler, sdk / 'Licenses', sdk / 'License', Path(os.environ['VSINSTALLDIR']) / 'Licenses']:
        if not root.is_dir():
            continue
        for path in sorted(root.rglob('*')):
            if (path.is_file() and path.suffix.lower() in {'.txt', '.rtf', '.html', '.htm'}
                    and any(word in path.name.lower() for word in ['license', 'notice', 'redist'])):
                relative = Path('notices') / str(len(receipt['license_documents'])) / path.name
                target = args.output / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
                receipt['license_documents'].append({'source': str(path), 'file': relative.as_posix(), 'sha256': digest(target)})
    receipt['publisher_terms'] = capture_terms(args.output)
    # Developer terms remain reference evidence in the source archive. Only
    # the end-user runtime terms accompany installed third-party notices.
    runtime = receipt['publisher_terms'][1]
    runtime_notice = Path('notices/Microsoft-C-Runtime.txt')
    (args.output / runtime_notice.parent).mkdir(parents=True, exist_ok=True)
    shutil.copy2(args.output / runtime['text_file'], args.output / runtime_notice)
    receipt['license_documents'].append({'source': runtime['url'], 'file': runtime_notice.as_posix(),
                                         'sha256': digest(args.output / runtime_notice)})
    receipt['visual_studio'] = json.loads(subprocess.check_output([
        str(Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'),
        '-latest', '-products', '*', '-format', 'json'], text=True))
    receipt['references'] = {
        'link_diagnostic': 'https://learn.microsoft.com/en-us/cpp/build/reference/link-repro-full-path-rsp?view=msvc-170',
        'visual_studio_terms': 'https://visualstudio.microsoft.com/license-terms/',
        'visual_studio_redist': 'https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution',
        'ucrt_deployment': 'https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment?view=msvc-170'}
    receipt['review'] = ('Input hashes and installed attribution evidence; platform/system-library '
                         'classification and applicable redistribution terms require review before publication')
    (args.output / 'platform-inventory.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
