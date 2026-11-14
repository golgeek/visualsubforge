export type OpenKind = 'subtitles' | 'media' | 'project' | 'legacy';
export interface OpenFile {
  path: string;
  kind: OpenKind;
}
const media = new Set([
  'mp4',
  'mkv',
  'mov',
  'avi',
  'webm',
  'm4v',
  'mpg',
  'mpeg',
  'ts',
  'm2ts',
  'wmv',
  'flv',
  'ogv',
  'wav',
  'mp3',
  'flac',
  'm4a',
  'aac',
  'ogg',
  'opus',
]);
export function classifyFiles(paths: string[]): OpenFile[] {
  if (!paths.length || paths.length > 2)
    throw new Error(
      'Open one project, or one video/audio file and one subtitle file.',
    );
  const files = paths.map((path): OpenFile => {
    const extension = path
      .split(/[\\/]/)
      .pop()
      ?.split('.')
      .pop()
      ?.toLowerCase();
    if (!path || path.includes('\0')) throw new Error('Invalid file path.');
    if (extension === 'vsfproject') return { path, kind: 'project' };
    if (extension === 'vssprj') return { path, kind: 'legacy' };
    if (['srt', 'ass', 'ssa', 'vtt'].includes(extension ?? ''))
      return { path, kind: 'subtitles' };
    if (media.has(extension ?? '')) return { path, kind: 'media' };
    throw new Error(
      'Unsupported file type. Choose SRT, ASS, SSA, VTT, a video/audio file, .vsfproject or .vssprj.',
    );
  });
  if (
    (files.length > 1 && new Set(files.map((file) => file.kind)).size !== 2) ||
    (files.length > 1 &&
      files.some((file) => file.kind === 'project' || file.kind === 'legacy'))
  )
    throw new Error(
      'Open a project by itself, or pair one video/audio file with one subtitle file.',
    );
  // Resolve unsaved subtitle work before changing media; cancellation stops the pair.
  return files.sort(
    (a, b) => Number(a.kind === 'media') - Number(b.kind === 'media'),
  );
}
