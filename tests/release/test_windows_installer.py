import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from windows_installer import verify


class InstallerProvenance(unittest.TestCase):
    def test_actual_embedded_plugin_bytes_and_toolset_must_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output, cache, installers = (root / p for p in ('build', 'NSIS', 'installers'))
            for path in (output / 'archives', output / 'plugin', cache / 'Plugins/x86-unicode', installers):
                path.mkdir(parents=True)
            helper, system = b'source-built helper', b'NSIS system plugin'
            plugin_path = output / 'plugin/nsis_tauri_utils.dll'
            plugin_path.write_bytes(helper)
            (output / 'installer-build.json').write_text(json.dumps({'status': 'passed', 'plugin': {
                'file': 'plugin/nsis_tauri_utils.dll', 'sha256': hashlib.sha256(helper).hexdigest()}}))
            system_path = cache / 'Plugins/x86-unicode/System.dll'
            system_path.write_bytes(system)
            with zipfile.ZipFile(output / 'archives/nsis-3.11.zip', 'w') as archive:
                archive.writestr('nsis-3.11/Plugins/x86-unicode/System.dll', system)
            (installers / 'preview.exe').write_bytes(b'opaque installer, inspected by 7z')
            payloads = {'$PLUGINSDIR\\nsis_tauri_utils.dll': helper, '$PLUGINSDIR\\System.dll': system}

            def inspect(command, **kwargs):
                if command[1] == 'l':
                    return '\n'.join('Path = ' + name for name in payloads)
                return payloads[command[-1]]

            with patch('windows_installer.subprocess.check_output', side_effect=inspect):
                result = verify(output, installers, cache)
                self.assertEqual(len(result['embedded_plugins_sha256']), 2)
                payloads['$PLUGINSDIR\\nsis_tauri_utils.dll'] = b'upstream prebuilt helper'
                with self.assertRaisesRegex(ValueError, 'Embedded installer plugin differs'):
                    verify(output, installers, cache)
                del payloads['$PLUGINSDIR\\nsis_tauri_utils.dll']
                with self.assertRaisesRegex(ValueError, 'was not found'):
                    verify(output, installers, cache)
                system_path.write_bytes(b'different NSIS toolset')
                with self.assertRaisesRegex(ValueError, 'NSIS cache differs'):
                    verify(output, installers, cache)
