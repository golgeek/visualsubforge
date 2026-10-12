"""Refresh the NSIS resource map after source/notice assembly."""
import json


def write_config(package, template=None):
    path = package / 'tauri.bundle.json'
    if path.exists():
        config = json.loads(path.read_text())
    else:
        config = {'bundle': {'active': True, 'targets': ['nsis'],
                            'license': 'GPL-3.0-or-later', 'shortDescription': 'Subtitle editor and resync CLI',
                            'windows': {'webviewInstallMode': {'type': 'downloadBootstrapper', 'silent': True},
                                        'nsis': {'installMode': 'currentUser'}}}}
    bundle = package / 'VisualSubForge'
    config['bundle']['resources'] = {str(file): file.relative_to(bundle).as_posix()
        for file in bundle.rglob('*') if file.is_file() and file.name != 'visualsubforge.exe'}
    if template:
        config['bundle']['windows']['nsis']['template'] = str(template)
    path.write_text(json.dumps(config, indent=2) + '\n')
