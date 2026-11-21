"""Check loader evidence even when an interposed library was never mapped."""
from pathlib import Path
import tempfile
import unittest

from linux_library_bindings import audit


class Bindings(unittest.TestCase):
    def test_mappings_and_bindings_must_agree_with_shipped_families(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            private = root / 'lib'
            private.mkdir()
            app = root / 'visualsubforge'
            mpv = private / 'libmpv.so.2'
            codec = private / 'libavcodec.so.62'
            webkit = root / 'libwebkit2gtk-4.1.so.0'
            for path in (mpv, codec):
                path.touch()

            def binding(caller, provider):
                return f"100: binding file {caller} [0] to {provider} [0]: normal symbol `fixture'"

            lines = [binding(app, mpv), binding(mpv, codec), binding(app, webkit)]
            mapped = {str(p) for p in (mpv, codec, webkit)}
            report = root / 'audit.json'
            audit(lines, mapped, private, report)
            for trace, extra in [
                (lines, {str(private / 'libavcodec.so.60')}),
                (lines + [binding(mpv, root / 'libavcodec.so.62')], set()),
                (lines + [binding(webkit, codec)], set()),
                ([], set()),
            ]:
                with self.assertRaises(ValueError):
                    audit(trace, mapped | extra, private, report)
