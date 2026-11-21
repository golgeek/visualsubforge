"""Apply the same synthetic plugin edit using either installed platform CLI."""
import hashlib
import json
import subprocess


def edited_srt(cli, fixtures, output):
    options = ['--subtitles', fixtures / 'subtitles/fansub.srt',
               '--package', fixtures / 'plugins/trim-trailing/manifest.json', '--json']
    review = json.loads(subprocess.check_output([cli, 'plugin', 'preview', *options], timeout=120))
    target = output / 'edited.srt'
    subprocess.check_output([cli, 'plugin', 'apply', *options, '--accept', review['reviewId'], '--output', target], timeout=120)
    data = target.read_bytes()
    if data == (fixtures / 'subtitles/fansub.srt').read_bytes():
        raise ValueError('The edit fixture did not exercise a subtitle change')
    return hashlib.sha256(data).hexdigest()
