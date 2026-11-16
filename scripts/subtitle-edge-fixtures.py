#!/usr/bin/env python3
"""Derive malformed/edge Matroska cases from our FFmpeg-generated synthetic clip.

Only mutate identified EBML subtitle blocks; video/audio and header stay intact.
No external media, fonts or captured private container bytes are used.
"""
from pathlib import Path
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / 'tests/fixtures/generated/subtitle-extraction'
original = (root / 'offset.mkv').read_bytes()


def vint(data, at, identifier=False):
    first = data[at]
    width = 1
    while width <= 8 and not first & (1 << (8 - width)):
        width += 1
    if width > 8:
        raise ValueError('Invalid EBML integer')
    value = int.from_bytes(data[at:at + width], 'big')
    return (value if identifier else value & ((1 << (7 * width)) - 1)), width


def elements(data, start, end):
    at = start
    while at < end:
        tag, tw = vint(data, at, True)
        size, sw = vint(data, at + tw)
        body = at + tw + sw
        finish = min(body + size, end)
        yield tag, at, body, finish
        at = finish


for mode in ['no-duration', 'empty', 'invalid-utf8']:
    data = bytearray(original)
    changed = 0
    for tag, _, segment, end in elements(original, 0, len(original)):
        if tag != 0x18538067:  # Segment
            continue
        for tag, _, cluster, cluster_end in elements(original, segment, end):
            if tag != 0x1F43B675:
                continue
            for tag, group_start, group, group_end in elements(original, cluster, cluster_end):
                if tag != 0xA0:  # BlockGroup
                    continue
                children = list(elements(original, group, group_end))
                blocks = [(body, finish) for tag, _, body, finish in children if tag == 0xA1]
                if not blocks:
                    continue
                block, finish = blocks[0]
                track, width = vint(original, block)
                if track != 3:  # stream 2, SRT
                    continue
                if mode == 'empty':
                    data[group_start] = 0xEC  # Void has same one-byte ID
                    changed += 1
                elif mode == 'no-duration':
                    for tag, at, _, _ in children:
                        if tag == 0x9B:
                            data[at] = 0xEC
                            changed += 1
                else:
                    data[block + width + 3] = 255  # first text byte after timestamp/flags
                    changed += 1
    if changed != 2:
        raise ValueError(f'{mode}: expected two synthetic SRT blocks, got {changed}')
    (root / f'{mode}.mkv').write_bytes(data)
    print(f'{mode}: changed {changed} subtitle records')

# Listing-only VobSub declaration: packets deliberately remain synthetic text,
# not decodable bitmap subtitles. Capture ffprobe metadata without decoding.
import subprocess
picture = root / 'picture-listing.mkv'
assert original.count(b'S_TEXT/UTF8') == 1
picture.write_bytes(original.replace(b'S_TEXT/UTF8', b'S_VOBSUB\0\0\0', 1))
listing = subprocess.check_output([
    'ffprobe', '-v', 'error', '-show_entries',
    'stream=index,codec_type,codec_name:stream_tags=language,title,filename,mimetype:stream_disposition=default,forced,hearing_impaired',
    '-of', 'json', str(picture),
])
(root / 'picture-track-listing.json').write_bytes(listing)
