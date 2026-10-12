from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from webview2_sources import verify_loaders


class WebviewLoaderSources(unittest.TestCase):
    def test_crate_binaries_must_match_the_sdk(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'sdk.nupkg'
            with zipfile.ZipFile(archive, 'w') as package:
                for architecture in ['x64', 'x86', 'arm64']:
                    (root / architecture).mkdir()
                    for name in ['WebView2Loader.dll', 'WebView2Loader.dll.lib', 'WebView2LoaderStatic.lib']:
                        path = root / architecture / name
                        path.write_bytes(f'{architecture}/{name}'.encode())
                        package.writestr('build/native/' + path.relative_to(root).as_posix(), path.read_bytes())
            self.assertEqual(len(verify_loaders(archive, root)), 9)
            (root / 'x64/WebView2LoaderStatic.lib').write_bytes(b'changed static input')
            with self.assertRaisesRegex(ValueError, 'Crate loader differs from Microsoft SDK'):
                verify_loaders(archive, root)
