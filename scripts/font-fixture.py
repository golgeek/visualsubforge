#!/usr/bin/env python3
"""Regenerate the original tiny font/ASS fixture with fontTools 4.61.1.

All outlines and metadata are authored here; no third-party font is copied.
Only fixture generation needs fontTools, not builds or the application.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen


def glyph(points):
    pen = TTGlyphPen(None)
    pen.moveTo(points[0])
    for point in points[1:]:
        pen.lineTo(point)
    pen.closePath()
    return pen.glyph()


def encode(data):
    # ASS's six-bit encoding uses ASCII33..96, no '=' padding, 80-column lines.
    out = []
    for at in range(0, len(data), 3):
        part = data[at:at + 3]
        value = int.from_bytes(part.ljust(3, b'\0'), 'big')
        out.extend(chr(33 + ((value >> shift) & 63))
                   for shift in (18, 12, 6, 0)[:len(part) + 1])
    text = ''.join(out)
    return '\n'.join(text[at:at + 80] for at in range(0, len(text), 80))


def main():
    root = Path(__file__).resolve().parent.parent / 'tests/fixtures/subtitles'
    font = FontBuilder(1000, isTTF=True)
    font.setupGlyphOrder(['.notdef', 'triangle', 'square'])
    font.setupCharacterMap({ord('A'): 'triangle', ord('B'): 'square'})
    font.setupGlyf({
        '.notdef': glyph([(50, 0), (50, 200), (150, 200), (150, 0)]),
        'triangle': glyph([(50, 0), (450, 0), (250, 700)]),
        'square': glyph([(50, 0), (450, 0), (450, 700), (50, 700)]),
    })
    font.setupHorizontalMetrics({name: (500, 50) for name in font.font.getGlyphOrder()})
    font.setupHorizontalHeader(ascent=800, descent=-200)
    font.setupNameTable({'familyName': 'VSF Fixture Font', 'styleName': 'Regular',
                        'uniqueFontIdentifier': 'VSF Fixture Font 1',
                        'fullName': 'VSF Fixture Font Regular',
                        'psName': 'VSFFixtureFont-Regular', 'version': 'Version 1.0',
                        'copyright': 'Original VisualSubForge test fixture; GPL-3.0-or-later'})
    font.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    font.setupPost()
    font.setupMaxp()
    font.font['head'].created = font.font['head'].modified = 2082844800
    font.font.recalcTimestamp = False
    path = root / 'fixture-font.ttf'
    font.save(path)
    source = (root / 'styles.ass').read_text().replace('sans-serif', 'VSF Fixture Font')
    source = source.replace('Red title', 'AB').replace('Second line\\N世界', 'BA')
    # Fonts last, so an unknown section cannot be mistaken for font bytes by libass.
    source += '\n[Fonts]\nfontname: VSFFixtureFont.ttf\n' + encode(path.read_bytes()) + '\n'
    (root / 'embedded-font.ass').write_text(source)
    print(f'Generated {path.stat().st_size}-byte original fixture font and embedded-font.ass')


if __name__ == '__main__':
    main()
