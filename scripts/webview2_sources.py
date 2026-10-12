"""Identify the prebuilt Microsoft loader shipped inside the locked Rust crate.

The crate's MIT metadata describes the Rust bindings. Microsoft's SDK has its
own notices; its binary payload is not represented as recovered loader source.
"""
import hashlib
from pathlib import Path
import shutil
import urllib.request
import zipfile

from native_evidence import digest

CRATE_VERSION = '0.39.1'
SDK_VERSION = '1.0.3800.47'
SDK_SHA256 = '56c9f26bdd07916a2d1949fb58a5c7e434dfa1173577dca879206050c4e718db'
SDK_URL = (f'https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/{SDK_VERSION}/'
           f'microsoft.web.webview2.{SDK_VERSION}.nupkg')


def verify_loaders(archive, crate):
    records = []
    with zipfile.ZipFile(archive) as package:
        for architecture in ['x64', 'x86', 'arm64']:
            for name in ['WebView2Loader.dll', 'WebView2Loader.dll.lib', 'WebView2LoaderStatic.lib']:
                relative = Path(architecture) / name
                sdk_path = f'build/native/{relative.as_posix()}'
                actual = digest(crate / relative)
                if hashlib.sha256(package.read(sdk_path)).hexdigest() != actual:
                    raise ValueError(f'Crate loader differs from Microsoft SDK: {relative}')
                records.append({'crate_path': relative.as_posix(), 'sdk_path': sdk_path, 'sha256': actual})
    return records


def collect(crate, version, target, output):
    if version != CRATE_VERSION:
        raise ValueError(f'Unreviewed WebView2 loader version: {version}')
    architecture = {'x86_64-pc-windows-msvc': 'x64', 'aarch64-pc-windows-msvc': 'arm64',
                    'i686-pc-windows-msvc': 'x86'}.get(target)
    if architecture is None:
        raise ValueError(f'Unreviewed WebView2 loader target: {target}')
    directory = output / 'sources/platform'
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / f'microsoft.web.webview2.{SDK_VERSION}.nupkg'
    with urllib.request.urlopen(SDK_URL, timeout=120) as response, archive.open('xb') as stream:
        shutil.copyfileobj(response, stream)
    if digest(archive) != SDK_SHA256:
        raise ValueError('Microsoft WebView2 SDK checksum mismatch')
    files = verify_loaders(archive, crate)
    notices = output / 'notices/platform/webview2'
    notices.mkdir(parents=True)
    with zipfile.ZipFile(archive) as package:
        for filename in ['LICENSE.txt', 'NOTICE.txt']:
            (notices / filename).write_bytes(package.read(filename))
    return {'name': 'Microsoft.Web.WebView2', 'version': SDK_VERSION,
            'url': SDK_URL, 'file': archive.name, 'sha256': SDK_SHA256,
            'license': 'BSD-3-Clause (SDK LICENSE.txt); additional SDK NOTICE.txt retained',
            'verified_crate_files': files,
            'selected_link_input': f'{architecture}/WebView2LoaderStatic.lib',
            'notices_sha256': {p.name: digest(p) for p in sorted(notices.iterdir())},
            'scope': 'Exact prebuilt loader and SDK notices; the SDK does not include loader implementation source. '
                     'The separately installed Evergreen WebView2 Runtime is not bundled in this archive.'}
