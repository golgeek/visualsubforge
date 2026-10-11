#!/usr/bin/env python3
"""Create an isolated, ad-hoc signed dependency-bundling PoC; not a release builder."""
import argparse
import hashlib
import json
import plistlib
import re
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source', type=Path, default=Path('target/debug/bundle/macos/VisualSubForge.app'))
parser.add_argument('--output', type=Path, default=Path('tests/fixtures/generated/portable/VisualSubForge.app'))
args = parser.parse_args()
if args.output.exists():
    raise SystemExit('Output already exists; choose a new output directory.')
source = args.source.resolve()
if args.output.resolve().is_relative_to(source):
    raise SystemExit('Output must be outside the source app.')
executable = source/'Contents/MacOS/visualsubforge'
def run(*argv):
    return subprocess.check_output(argv, text=True, stderr=subprocess.STDOUT)
def dependencies(path):
    return [line.strip().split(' (compatibility')[0] for line in run('otool','-L',str(path)).splitlines()[1:]]
def system(name):
    return name.startswith(('/System/Library/','/usr/lib/'))
# Audit before copying. The current Homebrew closure uses absolute install names;
# fail on unknown loader tokens instead of silently making an incomplete bundle.
queue = [executable]
closure = {}
edges = {}
while queue:
    path = queue.pop()
    edges[str(path)] = dependencies(path)
    for dep in edges[str(path)]:
        if system(dep):
            continue
        if not dep.startswith('/'):
            raise SystemExit(f'Unsupported dependency resolution: {path}: {dep}')
        resolved = Path(dep).resolve(strict=True)
        if resolved == path or str(resolved) in closure:
            continue
        name = resolved.name
        if any(p.name == name and p != resolved for p in closure.values()):
            raise SystemExit(f'Conflicting library basename: {name}')
        closure[str(resolved)] = resolved
        queue.append(resolved)
shutil.copytree(source, args.output)
frameworks = args.output/'Contents/Frameworks'
frameworks.mkdir(exist_ok=True)
records = []
for original in closure.values():
    dest = frameworks/original.name
    shutil.copy2(original, dest)
    dest.chmod(0o755)
    records.append({'source':str(original), 'file':original.name,
                    'sha256':hashlib.sha256(original.read_bytes()).hexdigest(),
                    'build_version':run('xcrun','vtool','-show-build',str(original))})
for original in [executable, *closure.values()]:
    dest = args.output/'Contents/MacOS/visualsubforge' if original == executable else frameworks/original.name
    for dep in edges[str(original)]:
        if not system(dep):
            replacement = '@executable_path/../Frameworks/'+Path(dep).resolve().name
            subprocess.run(['install_name_tool','-change',dep,replacement,str(dest)],check=True)
    if original != executable:
        subprocess.run(['install_name_tool','-id','@executable_path/../Frameworks/'+original.name,str(dest)],check=True)
    # Fail if an absolute third-party runtime dependency survived relocation.
    for dep in dependencies(dest):
        if not system(dep) and not dep.startswith('@executable_path/../Frameworks/'):
            raise SystemExit(f'Unrelocated library: {dep}')
    subprocess.run(['codesign','--force','--sign','-',str(dest)],check=True)
# A lower Info.plist value cannot make newer dependencies run on older macOS.
minimums = [v for r in records for v in re.findall(r'minos\s+(\S+)', r['build_version'])]
info_path = args.output/'Contents/Info.plist'
info = plistlib.loads(info_path.read_bytes())
minimums.append(info.get('LSMinimumSystemVersion', '11.0'))
info['LSMinimumSystemVersion'] = max(minimums, key=lambda v: tuple(map(int,v.split('.'))))
info_path.write_bytes(plistlib.dumps(info))
subprocess.run(['codesign','--force','--sign','-',str(args.output)],check=True)
subprocess.run(['codesign','--verify','--deep','--strict',str(args.output)],check=True)
(args.output.parent/'dependency-manifest.json').write_text(json.dumps(records,indent=2)+'\n')
print(f'Bundled {len(records)} libraries; ad-hoc signature verified. No signing/notarization or redistribution clearance implied.')
