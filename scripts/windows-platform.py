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

from native_evidence import digest


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
