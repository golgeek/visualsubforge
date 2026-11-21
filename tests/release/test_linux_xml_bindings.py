"""Actual consumer/provider pairs matter when both XML ABIs are mapped."""
import json
from pathlib import Path
import tempfile
import unittest

from linux_xml_bindings import audit


class XmlBindings(unittest.TestCase):
    def test_private_and_system_consumers_must_bind_their_own_xml_abi(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            private = root / 'lib'
            old, new = private / 'libxml2.so.2', root / 'libxml2.so.16'
            ffmpeg, webkit = private / 'libavformat.so.60', root / 'libwebkit2gtk-4.1.so.0'
            report = root / 'report.json'
            for provider1, provider2 in [(old, new), (new, new), (old, old)]:
                lines = [f"100: binding file {c} [0] to {p} [0]: normal symbol `xmlReadMemory'"
                         for c, p in [(ffmpeg, provider1), (webkit, provider2)]]
                args = (lines, {str(old), str(new)}, private, report)
                if (provider1, provider2) == (old, new):
                    audit(*args)
                    self.assertEqual(json.loads(report.read_text())['failures'], [])
                else:
                    with self.assertRaises(ValueError):
                        audit(*args)
                    self.assertTrue(json.loads(report.read_text())['failures'])
            with self.assertRaises(ValueError):
                audit([], {str(old), str(new)}, private, report)
