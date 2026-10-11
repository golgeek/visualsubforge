#!/usr/bin/env python3
"""Fetch an unchanged Blender reference movie into ignored local fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--resolution', choices=['180','1080'], default='180')
args = parser.parse_args()
archive, member, target = {
    '180': ('BigBuckBunny_320x180.mp4.zip','BigBuckBunny_320x180.mp4','big-buck-bunny.mp4'),
    '1080': ('big_buck_bunny_1080p_h264.mov.zip','big_buck_bunny_1080p_h264.mov','big-buck-bunny-1080.mov'),
}[args.resolution]
out = Path(__file__).resolve().parent.parent/'tests/fixtures/generated'
out.mkdir(parents=True, exist_ok=True)
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
def sha256(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f,'sha256').hexdigest()
(out/f'bbb-{args.resolution}-provenance.json').write_text(json.dumps({
    'source':url,'author':'(c) copyright 2008, Blender Foundation / www.bigbuckbunny.org',
    'license':'CC BY 3.0','license_source':'https://peach.blender.org/about/',
    'zip_sha256':sha256(path),'media_sha256':sha256(out/target),'modified':False,
},indent=2)+'\n')
print(out/target)
