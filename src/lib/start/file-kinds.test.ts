import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyFiles } from './file-kinds.ts';
test('opening classifies supported files and orders subtitle protection before media', () => {
  assert.deepEqual(
    classifyFiles(['C:\\clips\\MOVIE.MKV', '/captions.EN.VTT']),
    [
      { path: '/captions.EN.VTT', kind: 'subtitles' },
      { path: 'C:\\clips\\MOVIE.MKV', kind: 'media' },
    ],
  );
  for (const [path, kind] of [
    ['a.vsfproject', 'project'],
    ['a.VSSPRJ', 'legacy'],
    ['a.ass', 'subtitles'],
    ['a.flac', 'media'],
  ])
    assert.equal(classifyFiles([path!])[0]?.kind, kind);
});
test('opening refuses ambiguous groups before opening any file', () => {
  for (const files of [
    [],
    ['a.srt', 'b.srt'],
    ['a.mkv', 'b.mp4'],
    ['a.vsfproject', 'a.srt'],
    ['a.vssprj', 'a.mp4'],
    ['a.mp4', 'b.srt', 'c.ass'],
    ['script.exe'],
    ['a\0.srt'],
  ])
    assert.throws(() => classifyFiles(files));
});
