"""Build and inventory the code embedded by the Windows NSIS installer.

The upstream helper release has no retained Cargo.lock. Build its pinned source
with our lock/toolchain and select it through one generated template change.
The ordinary Tauri cache and its upstream hash verification remain untouched.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

from dependency_sources import source_notices, verified_download, verify_unpacked_crate
from locked_crate_sources import collect as collect_locked
from native_evidence import digest
from rust_runtime_sources import collect as collect_runtime
from windows_bundle import write_config
from windows_link_inputs import input_paths
from windows_notices import check_nsis, compact

ROOT = Path(__file__).resolve().parent.parent
PINS = ROOT / 'tests/native/nsis-installer'
TARGET = 'i686-pc-windows-msvc'
PLUGIN = 'nsis_tauri_utils.dll'
TEMPLATE_DIRECTIVE = '!addplugindir "${ADDITIONALPLUGINSPATH}"'


def select_plugin_template(original):
    if original.count(TEMPLATE_DIRECTIVE) != 1:
        raise ValueError('Pinned installer template plugin selection changed')
    return original.replace(TEMPLATE_DIRECTIVE, '\n'.join([
        '; VSF builds this helper from pinned source, Cargo.lock and Rust.',
        '!if "$%VSF_NSIS_PLUGIN_DIR%" == ""',
        '  !error "VSF_NSIS_PLUGIN_DIR is required for the inventoried helper"',
        '!endif', '!addplugindir "$%VSF_NSIS_PLUGIN_DIR%"']))


def prepare(output, cached=None):
    """Collect immutable inputs; also callable on non-Windows hosts for review."""
    output.mkdir(parents=True, exist_ok=False)
    pins = json.loads((PINS / 'sources.json').read_text())
    if json.loads((ROOT / 'package.json').read_text())['devDependencies']['@tauri-apps/cli'] != pins['tauri_cli']:
        raise ValueError('Review installer inputs for the new Tauri CLI version')
    archives = output / 'archives'
    archives.mkdir()
    receipt = {'schema': 1, 'status': 'prepared', 'pins': pins, 'archives': {}, 'notices': {}}
    for name, source in pins['archives'].items():
        path = archives / name
        receipt['archives'][name] = verified_download(source['url'], path, 'sha256',
            bytes.fromhex(source['sha256']), cached / name if cached else None)
        if not name.endswith('.zip'):
            notices = source_notices(path, output / 'notices' / name)
            if not notices:
                raise ValueError(f'No installer source notices: {name}')
            receipt['notices'][name] = notices
    receipt['rust_dependencies'] = collect_locked((PINS / 'Cargo.lock').read_bytes(), output / 'rust')
    recipe = output / 'recipe'
    recipe.mkdir()
    for name in ['sources.json', 'Cargo.lock', 'README.md', 'allocator-control.rs']:
        shutil.copy2(PINS / name, recipe)
    receipt['patches'] = {}
    for patch in sorted(PINS.glob('*.patch')):
        shutil.copy2(patch, recipe)
        receipt['patches'][patch.name] = digest(patch)
    for name in ['windows_installer.py', 'windows_bundle.py', 'windows_link_inputs.py', 'windows_notices.py',
                 'rust_runtime_sources.py', 'dependency_sources.py', 'locked_crate_sources.py',
                 'native_evidence.py', 'source_notices.py']:
        shutil.copy2(ROOT / 'scripts' / name, recipe)
    shutil.copy2(ROOT / 'rust-toolchain.toml', recipe)
    shutil.copy2(ROOT / 'LICENSE', recipe / 'VSF-LICENSE')
    with tarfile.open(archives / 'tauri-cli-source.tar.gz') as archive:
        prefix = 'tauri-' + pins['tauri_commit'] + '/crates/tauri-bundler/src/bundle/windows/nsis/'
        original = archive.extractfile(prefix + 'installer.nsi').read().decode('utf-8')
        bundler = archive.extractfile(prefix + 'mod.rs').read().decode('utf-8')
        if pins['archives']['nsis-3.11.zip']['url'] not in bundler:
            raise ValueError('Pinned bundler selected a different NSIS version')
    (recipe / 'original-installer.nsi').write_text(original, encoding='utf-8')
    (recipe / 'installer.nsi').write_text(select_plugin_template(original), encoding='utf-8')
    receipt['template'] = {'original_sha256': digest(recipe / 'original-installer.nsi'),
                           'selected_sha256': digest(recipe / 'installer.nsi'),
                           'change': 'Select the source-built helper through VSF_NSIS_PLUGIN_DIR'}
    return receipt


def build(output):
    if os.name != 'nt':
        raise ValueError('Build the NSIS helper on Windows with the x86 MSVC environment')
    receipt = prepare(output)
    receipt['status'] = 'incomplete'
    work = output / 'work'
    work.mkdir()
    with tarfile.open(output / 'archives/nsis-tauri-utils-source.tar.gz') as archive:
        archive.extractall(work, filter='data')
    source = work / ('nsis-tauri-utils-' + receipt['pins']['plugin_commit'])
    shutil.copy2(PINS / 'Cargo.lock', source / 'Cargo.lock')
    allocator = source / 'crates/nsis-plugin-api/src/lib.rs'
    with allocator.open('a', encoding='utf-8') as file:
        file.write('\n' + (PINS / 'allocator-control.rs').read_text(encoding='utf-8'))
    channel = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    env = dict(os.environ)
    env.pop('RUSTFLAGS', None)
    env['RUST_BACKTRACE'] = '1'
    flags = ['-C', 'target-feature=+crt-static']
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    receipt['rustflags'] = flags
    commands = []
    receipt['commands'] = commands
    try:
        with (output / 'commands.log').open('w', encoding='utf-8') as log:
            with zipfile.ZipFile(output / 'archives/nsis-3.11.zip') as archive:
                archive.extractall(output / 'nsis-control')
            receipt['notice_layout_control'] = check_nsis(
                output, output / 'nsis-control/nsis-3.11/makensis.exe', log)
            def run(*args, check=True):
                args = list(map(str, args))
                commands.append(args)
                print('RUN', json.dumps(args), flush=True)
                return subprocess.run(args, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT, check=check)
            run('rustup', 'target', 'add', '--toolchain', channel, TARGET)
            control = run('cargo', '+' + channel, 'test', '--locked', '-p', 'nsis-plugin-api',
                '--target', TARGET, '--target-dir', output / 'target', 'vsf_allocator_control',
                '--', '--nocapture', '--test-threads=1', check=False)
            log.flush()
            diagnostics = (output / 'commands.log').read_text(encoding='utf-8')
            if control.returncode == 0 or 'alignment=' not in diagnostics or '1 failed' not in diagnostics:
                raise ValueError('Unpatched allocator did not reproduce the expected alignment failure')
            receipt['unpatched_allocator_control'] = {'exit': control.returncode,
                'result': 'Requested alignment violated; expected failure before patch'}
            for patch in sorted(PINS.glob('*.patch')):
                run('git', 'apply', '--check', patch)
                run('git', 'apply', patch)
            tests = ['cargo', '+' + channel, 'test', '--locked', '--workspace', '--features', 'test',
                     '--target', TARGET, '--target-dir', output / 'target', '--no-fail-fast']
            # NSIS calls these plugins serially. Their tests share a process
            # token (temporary privileges) and the global NSIS stack adapter.
            # Keep all assertions; separate test bodies must not race this state.
            run(*tests, '--', '--nocapture', '--test-threads=1')
            # Only the shipped DLL owns this receipt. Applying the option to
            # workspace tests makes their concurrent linkers race for one file.
            run('cargo', '+' + channel, 'rustc', '--locked', '--release', '--lib', '--target', TARGET,
                '--target-dir', output / 'target', '-p', 'nsis-tauri-utils', '--', '-C',
                'link-arg=/LINKREPROFULLPATHRSP:' + str(output / 'helper.rsp'))
        metadata = json.loads(subprocess.check_output(['cargo', '+' + channel, 'metadata', '--locked',
            '--format-version', '1', '--filter-platform', TARGET], cwd=source, env=env, encoding='utf-8'))
        for package in metadata['packages']:
            if package['source']:
                verify_unpacked_crate(output / 'rust/sources/rust' / f"{package['name']}-{package['version']}.crate",
                                      Path(package['manifest_path']).parent)
        plugin = output / 'plugin' / PLUGIN
        plugin.parent.mkdir()
        shutil.copy2(output / 'target' / TARGET / 'release' / PLUGIN, plugin)
        import pefile
        with pefile.PE(str(plugin)) as pe:
            if pe.FILE_HEADER.Machine != 0x14c:
                raise ValueError('NSIS requires an i686 helper DLL')
            imports = sorted({entry.dll.decode('ascii').lower() for entry in pe.DIRECTORY_ENTRY_IMPORT})
        receipt['plugin'] = {'file': 'plugin/' + PLUGIN, 'sha256': digest(plugin), 'imports': imports}
        if set(imports) - {'kernel32.dll', 'user32.dll', 'advapi32.dll', 'shell32.dll'}:
            raise ValueError(f'Review unexpected helper runtime imports: {imports}')
        sysroot = Path(subprocess.check_output(['rustc', '+' + channel, '--print', 'sysroot'], encoding='utf-8').strip())
        receipt['rustc'] = subprocess.check_output(['rustc', '+' + channel, '-vV'], encoding='utf-8').strip()
        receipt['rust_runtime'] = collect_runtime(output / 'runtime', TARGET, sysroot)
        # os.environ normalizes Windows keys to uppercase. The subprocess copy
        # is an ordinary dict, so its lookups no longer normalize mixed case.
        compiler, sdk = (Path(env[name.upper()]).resolve(strict=True)
                         for name in ['VCToolsInstallDir', 'WindowsSdkDir'])
        receipt['platform_toolchain'] = {name: env[name.upper()]
                                        for name in ['VCToolsVersion', 'WindowsSDKVersion']}
        response = (output / 'helper.rsp').read_bytes()
        encoding = 'utf-16' if response.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig'
        receipt['link_inputs'] = []
        for item in input_paths(response.decode(encoding)):
            path = Path(item).resolve()
            origin = 'MSVC' if path.is_relative_to(compiler) else 'Windows SDK' if path.is_relative_to(sdk) else 'Rust/helper build'
            checksum = digest(path) if path.is_file() else None
            if checksum is None and origin != 'Rust/helper build':
                raise ValueError(f'Installer platform input is unavailable: {path}')
            receipt['link_inputs'].append({'path': str(path), 'origin': origin, 'sha256': checksum})
        receipt['status'] = 'passed'
    finally:
        (output / 'installer-build.json').write_text(json.dumps(receipt, indent=2) + '\n')


def attach(output, package):
    receipt = json.loads((output / 'installer-build.json').read_text())
    if receipt['status'] != 'passed' or digest(output / receipt['plugin']['file']) != receipt['plugin']['sha256']:
        raise ValueError('Installer helper requires a successful, unchanged build')
    target = package / 'native-sources/installer'
    target.mkdir(parents=True, exist_ok=False)
    for directory in ['archives', 'notices', 'recipe', 'rust', 'runtime', 'plugin']:
        shutil.copytree(output / directory, target / directory)
    for name in ['installer-build.json', 'commands.log', 'helper.rsp']:
        shutil.copy2(output / name, target)
    bundle = package / 'VisualSubForge'
    notices = bundle / 'third-party/installer'
    for directory in ['notices', 'rust/notices', 'runtime/notices']:
        shutil.copytree(target / directory, notices / directory)
    shutil.copy2(target / 'installer-build.json', bundle / 'installer-inventory.json')
    compact(bundle)
    notice_index = bundle / 'third-party/index.json'
    shutil.copy2(notice_index, package / 'native-sources/installed-notices.json')
    inventory_path = package / 'native-sources/native-inventory.json'
    inventory = json.loads(inventory_path.read_text())
    inventory['installer'] = {'inventory': 'installer/installer-build.json',
                               'sha256': digest(target / 'installer-build.json')}
    inventory['installed_notices'] = {'inventory': 'installed-notices.json',
                                      'sha256': digest(notice_index)}
    inventory_path.write_text(json.dumps(inventory, indent=2) + '\n')
    shutil.copy2(inventory_path, bundle / 'native-inventory.json')
    assembly = json.loads((package / 'assembly-receipt.json').read_text())
    assembly['native_inventory_sha256'] = digest(bundle / 'native-inventory.json')
    assembly['installer_inventory_sha256'] = digest(bundle / 'installer-inventory.json')
    assembly['notice_index_sha256'] = digest(notice_index)
    for path in [package / 'assembly-receipt.json', bundle / 'native-runtime.json']:
        path.write_text(json.dumps(assembly, indent=2) + '\n')
    native = package / 'native-sources'
    (native / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.relative_to(native).as_posix()}\n'
        for p in sorted(native.rglob('*')) if p.is_file() and p != native / 'SHA256SUMS'))
    write_config(package, output / 'recipe/installer.nsi')


def verify(output, installers, nsis_cache):
    """Bind the actual EXE to our helper and the pinned NSIS toolset/plugins."""
    receipt = json.loads((output / 'installer-build.json').read_text())
    if receipt['status'] != 'passed' or digest(output / receipt['plugin']['file']) != receipt['plugin']['sha256']:
        raise ValueError('Installer verification requires a successful, unchanged helper build')
    with zipfile.ZipFile(output / 'archives/nsis-3.11.zip') as archive:
        plugins, checked = {}, 0
        for item in archive.infolist():
            if item.is_dir():
                continue
            path = Path(*Path(item.filename).parts[1:])
            expected = hashlib.sha256(archive.read(item)).hexdigest()
            if digest(nsis_cache / path) != expected:
                raise ValueError(f'NSIS cache differs from the pinned distribution: {path}')
            checked += 1
            if path.as_posix().startswith('Plugins/x86-unicode/'):
                plugins[path.name.lower()] = expected
    plugins[PLUGIN] = receipt['plugin']['sha256']
    files = list(installers.glob('*.exe'))
    if len(files) != 1:
        raise ValueError(f'Expected one NSIS installer: {files}')
    installer = files[0]
    listing = subprocess.check_output(['7z', 'l', '-slt', '-sccUTF-8', installer], encoding='utf-8')
    embedded = {}
    for line in listing.splitlines():
        if not line.startswith('Path = '):
            continue
        name = line.removeprefix('Path = ')
        normalized = name.replace('\\', '/')
        if not normalized.startswith('$PLUGINSDIR/') or not normalized.lower().endswith('.dll'):
            continue
        payload = subprocess.check_output(['7z', 'e', '-so', installer, name])
        checksum = hashlib.sha256(payload).hexdigest()
        key = normalized.rsplit('/', 1)[-1].lower()
        if key in embedded or plugins.get(key) != checksum:
            raise ValueError(f'Embedded installer plugin differs from inventoried bytes: {name}')
        embedded[key] = checksum
    if PLUGIN not in embedded:
        raise ValueError('Source-built helper was not found in the actual installer')
    result = {'status': 'passed', 'installer': installer.name, 'installer_sha256': digest(installer),
              'build_inventory_sha256': digest(output / 'installer-build.json'),
              'nsis_cache_files_verified': checked, 'embedded_plugins_sha256': embedded}
    (installers / 'installer-receipt.json').write_text(json.dumps(result, indent=2) + '\n')
    return result
