#!/usr/bin/env python3
"""Mac software font controls using production preview packets/fallback helper."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    sources = Path(__file__).resolve().parent.parent / 'tests/fixtures/generated/fonts/sources'
    receipt = {'scope': 'Mac software composition; not desktop, GPU, audio or platform acceptance',
               'probe': str(args.probe.resolve()),
               'probe_sha256': hashlib.sha256(args.probe.read_bytes()).hexdigest(), 'cases': {}}
    for name, source, mode in [
        ('baseline', 'generic', 'baseline'), ('generic', 'generic', 'fallback'),
        ('explicit', 'explicit', 'fallback'), ('missing', 'missing', 'fallback'),
        ('embedded', 'embedded', 'embedded'),
        ('embedded-cff', 'embedded-cff', 'embedded'),
        ('embedded-cff2', 'embedded-cff2', 'embedded'),
        ('without-attachment', 'without-attachment', 'embedded'),
        ('missing-glyph', 'missing-glyph', 'fallback'),
    ]:
        folder = args.output / name
        folder.mkdir()
        path = sources / f'{source}.url'
        result = subprocess.run([str(args.probe.resolve()), mode, str(path), str(folder.resolve())],
                                check=False, capture_output=True, text=True, timeout=40)
        log = result.stdout + result.stderr
        (folder / 'native.log').write_text(log)
        result.check_returncode()
        for line in log.splitlines():
            if line.startswith('private-resource='):
                assert not Path(line.split('=', 1)[1]).exists(), 'Player leaked private resource'
        png = folder / '00000001.png'
        rgb = subprocess.run(['ffmpeg', '-v', 'error', '-i', str(png), '-f', 'rawvideo',
                              '-pix_fmt', 'rgb24', '-'], check=True, capture_output=True, timeout=30).stdout
        assert len(rgb) == 768 * 576 * 3
        lit = sum(max(rgb[i:i + 3]) > 30 for i in range(0, len(rgb), 3))
        receipt['cases'][name] = {'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                                  'image_sha256': hashlib.sha256(png.read_bytes()).hexdigest(),
                                  'lit_pixels': lit, 'notices': result.stdout.split('font-notices:\n')[-1]}
        if name == 'baseline':
            assert 'failed to find any fallback with glyph 0x4E16' in log
        elif name == 'generic':
            assert 'Using default font:' in log and 'failed to find any fallback with glyph 0x4E16' not in log
        elif name == 'explicit':
            assert 'HiraginoSansGB-' in log and 'Using default font:' not in log
        elif name == 'missing':
            assert 'Using default font family: (VSF Definitely Missing Font' in receipt['cases'][name]['notices']
        elif name == 'embedded':
            assert 'VSFFixtureFont-Regular' in log and lit > 100
        elif name in ('embedded-cff', 'embedded-cff2'):
            variant = 'CFF2' if name.endswith('cff2') else 'CFF'
            assert f'VSFFixtureFont-{variant}Regular' in log and lit > 100
            assert 'failed to find any fallback' not in log
        elif name == 'without-attachment':
            assert 'failed to find any fallback' in log and lit == 0
        elif name == 'missing-glyph':
            assert '10FFFF' in receipt['cases'][name]['notices'].upper(), 'Missing glyph absent from notices'
    assert receipt['cases']['generic']['image_sha256'] != receipt['cases']['baseline']['image_sha256']
    assert receipt['cases']['embedded-cff']['image_sha256'] != receipt['cases']['embedded']['image_sha256']
    assert receipt['cases']['embedded-cff2']['image_sha256'] != receipt['cases']['embedded']['image_sha256']
    receipt['passed'] = True
    (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(args.output / 'receipt.json')


if __name__ == '__main__':
    main()
