#!/usr/bin/env python3
"""Fetch an unchanged Blender reference movie into ignored local fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile


def sha256(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def fetch_tears_of_steel(out):
    url = 'https://download.blender.org/demo/movies/ToS/tears_of_steel_720p.mov'
    expected = 'efa9062d9cdb7a338e40ad530dfdf234806743f29ae6a1a136b97ece4e588e8f'
    target = out / 'tears-of-steel-720.mov'
    if not target.exists():
        partial = target.with_suffix('.mov.download')
        # A bounded attempt can be resumed by rerunning this script. A complete
        # partial file is verified before publication, even after interruption.
        if not partial.exists() or partial.stat().st_size != 372178639:
            subprocess.run([
                'curl', '--fail', '--silent', '--show-error', '--location',
                '--continue-at', '-', '--max-time', '600', '--max-filesize', '372178639',
                url, '-o', str(partial),
            ], check=True, timeout=610)
        if sha256(partial) != expected:
            raise RuntimeError(f'Reference movie hash mismatch; retained {partial}')
        # Do not replace a concurrently created destination.
        target.hardlink_to(partial)
        partial.unlink()
    if sha256(target) != expected:
        raise RuntimeError(f'Reference movie hash mismatch; preserved {target}')
    (out / 'tos-720-provenance.json').write_text(json.dumps({
        'source': url, 'author': '(CC) Blender Foundation | mango.blender.org',
        'license': 'CC BY 3.0', 'license_source': 'https://mango.blender.org/sharing/',
        'media_sha256': expected, 'modified': False,
    }, indent=2) + '\n')
    print(target)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--film', choices=['big-buck-bunny', 'tears-of-steel'], default='big-buck-bunny')
parser.add_argument('--resolution', choices=['180', '720', '1080'])
args = parser.parse_args()
out = Path(__file__).resolve().parent.parent/'tests/fixtures/generated'
out.mkdir(parents=True, exist_ok=True)
if args.film == 'tears-of-steel':
    if args.resolution not in (None, '720'):
        parser.error('Tears of Steel is available here at resolution 720 only')
    fetch_tears_of_steel(out)
    raise SystemExit(0)
if args.resolution == '720':
    parser.error('Big Buck Bunny is available here at resolution 180 or 1080')
args.resolution = args.resolution or '180'
archive, member, target = {
    '180': ('BigBuckBunny_320x180.mp4.zip','BigBuckBunny_320x180.mp4','big-buck-bunny.mp4'),
    '1080': ('big_buck_bunny_1080p_h264.mov.zip','big_buck_bunny_1080p_h264.mov','big-buck-bunny-1080.mov'),
}[args.resolution]
url = 'https://download.blender.org/peach/bigbuckbunny_movies/'+archive
path = out/archive
if not path.exists():
    partial = path.with_suffix('.download')
    # Use the system HTTPS client; this public archive rejected urllib's client.
    subprocess.run(['curl', '--fail', '--silent', '--show-error', '--location',
                    '--max-time', '180', '--max-filesize', str(800*1024*1024),
                    url, '-o', str(partial)], check=True, timeout=190)
    partial.replace(path)
with zipfile.ZipFile(path) as z:
    info = z.getinfo(member)
    if info.file_size > 1024*1024*1024:
        raise RuntimeError('Movie exceeded the expected size ceiling')
    # Extract just the expected member to a fixed destination, never archive paths.
    with z.open(member) as source, (out/target).open('wb') as dest:
        while chunk := source.read(1024*1024):
            dest.write(chunk)
(out/f'bbb-{args.resolution}-provenance.json').write_text(json.dumps({
    'source':url,'author':'(c) copyright 2008, Blender Foundation / www.bigbuckbunny.org',
    'license':'CC BY 3.0','license_source':'https://peach.blender.org/about/',
    'zip_sha256':sha256(path),'media_sha256':sha256(out/target),'modified':False,
},indent=2)+'\n')
print(out/target)
