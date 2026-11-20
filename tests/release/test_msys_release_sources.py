import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from msys_release_sources import PREFIX, audit_sources, mingw_snapshot, retention


class ReleaseSources(unittest.TestCase):
    def test_shipped_and_copyleft_sources_survive_shared_archives(self):
        packages = []
        sources = {}

        def add(name, source, license, shipped=False, snapshot=None):
            packages.append({'name': PREFIX + name, 'source': source, 'license': [license],
                             'required_by': ['shipped DLL'] if shipped else ['owner']})
            sources[source] = {'sha256': hashlib.sha256(b'source').hexdigest(), 'mingw_snapshot': snapshot}

        snapshot = {'commit': 'a' * 40, 'sha256': 'b' * 64}
        add('headers', 'headers.src', 'spdx:ZPL-2.1 AND LGPL-2.1-or-later', snapshot=snapshot)
        add('crt', 'crt.src', 'spdx:ZPL-2.1', snapshot=snapshot)
        add('crt', 'different.src', 'spdx:ZPL-2.1', snapshot={**snapshot, 'sha256': 'c' * 64})
        add('winpthreads', 'threads.src', 'spdx:MIT AND BSD-3-Clause', snapshot=snapshot)
        add('libwinpthread', 'threads.src', 'spdx:MIT AND BSD-3-Clause', shipped=True, snapshot=snapshot)
        add('xxhash', 'xxhash.src', 'spdx:BSD-2-Clause AND GPL-2.0-or-later')
        add('compiler-rt', 'llvm.src', 'spdx:Apache-2.0 WITH LLVM-exception')
        add('rust', 'rust.src', 'unreviewed')
        decisions = retention(packages, sources)
        self.assertEqual({name for name, rule in decisions.items() if not rule['included']},
                         {'crt.src', 'llvm.src'})
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, rule in decisions.items():
                if rule['included']:
                    (root / name).write_bytes(b'source')
            audit_sources(root, packages, sources)
            (root / 'threads.src').unlink()
            with self.assertRaisesRegex(ValueError, 'required MSYS2 source archive: threads.src'):
                audit_sources(root, packages, sources)

    def test_recipe_checksum_must_identify_the_upstream_source(self):
        commit, checksum = 'a' * 40, 'b' * 64
        recipe = (f"_commit='{commit}'\n"
                  'source=("mingw-w64"::"git+https://git.code.sf.net/p/mingw-w64/mingw-w64#commit=${_commit}"\n'
                  "  'local.patch')\n"
                  f"sha256sums=('{checksum}'\n  '{'c' * 64}')\n")
        self.assertEqual(mingw_snapshot(recipe), {'commit': commit, 'sha256': checksum})
        for bad in (recipe.replace(checksum, 'SKIP'), recipe.replace('git.code.sf.net', 'example.org'),
                    recipe.replace("'local.patch'", "'extra' 'patch'"), recipe.replace(checksum, '')):
            self.assertIsNone(mingw_snapshot(bad))
