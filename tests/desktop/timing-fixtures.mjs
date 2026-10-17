import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
const run = promisify(execFile);
export const time = (ms) =>
  `${[Math.floor(ms / 3600000), Math.floor(ms / 60000) % 60, Math.floor(ms / 1000) % 60].map((n) => String(n).padStart(2, '0')).join(':')},${String(ms % 1000).padStart(3, '0')}`;
export async function identity(path) {
  const bytes = await readFile(path);
  const info = await stat(path, { bigint: true });
  let hash = 0xcbf29ce484222325n;
  for (const offset of [
    0,
    Math.floor(bytes.length / 2),
    Math.max(0, bytes.length - 65536),
  ]) {
    for (const byte of bytes.subarray(offset, offset + 65536))
      hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  }
  return {
    size: bytes.length,
    modified: `${info.mtimeNs / 1000000000n}.${String(info.mtimeNs % 1000000000n).padStart(9, '0')}`,
    sampleHash: hash.toString(16).padStart(16, '0'),
  };
}

export async function makeVideos(root, stem, segments) {
  const small = join(root, `${stem}-small.mkv`),
    large = join(root, `${stem}-large.mkv`);
  const inputs = segments.flatMap(([color, duration]) => [
    '-f',
    'lavfi',
    '-i',
    `color=c=${color}:s=160x90:r=24:d=${duration}`,
  ]);
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      ...inputs,
      '-filter_complex',
      `${segments.map((_, i) => `[${i}:v]`).join('')}concat=n=${segments.length}:v=1:a=0[out]`,
      '-map',
      '[out]',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-threads',
      '1',
      small,
    ],
    { timeout: 30000 },
  );
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      '-f',
      'lavfi',
      '-i',
      "nullsrc=s=64x64:r=5/7,geq=lum='16+219*mod(N,2)':cb=128:cr=128",
      '-frames:v',
      '10001',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-g',
      '1',
      '-threads',
      '1',
      large,
    ],
    { timeout: 60000 },
  );
  return { small, large };
}
