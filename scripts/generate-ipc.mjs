// Run the Rust exporter without starting a desktop window or modifying sources
// during ordinary builds/tests. Check mode never writes generated repo files.
import { spawnSync } from 'node:child_process';
import { mkdtemp, readFile, rm, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { format, resolveConfig } from 'prettier';

const root = fileURLToPath(new URL('../', import.meta.url));
const check = process.argv.slice(2).includes('--check');
if (process.argv.slice(2).some((argument) => argument !== '--check')) {
  throw new Error('Usage: node scripts/generate-ipc.mjs [--check]');
}
const temporary = await mkdtemp(join(tmpdir(), 'vsf-ipc-'));
try {
  const result = spawnSync(
    'cargo',
    [
      'test',
      '--locked',
      '-p',
      'visualsubforge',
      'ipc::export_bindings',
      '--',
      '--ignored',
      '--exact',
    ],
    {
      cwd: root,
      env: { ...process.env, VSF_BINDINGS_DIR: temporary },
      stdio: 'inherit',
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error('Rust IPC export failed');

  for (const [name, destination] of [
    ['ipc.ts', 'src/lib/generated/ipc.ts'],
    ['ipc-contract-samples.ts', 'tests/frontend/ipc-contract-samples.ts'],
  ]) {
    const path = join(root, destination);
    const text = await format(await readFile(join(temporary, name), 'utf8'), {
      ...(await resolveConfig(path)),
      filepath: path,
    });
    if (check) {
      const current = await readFile(path, 'utf8').catch((error) => {
        if (error.code === 'ENOENT') return null;
        throw error;
      });
      if (text !== current) {
        throw new Error(`${destination} is stale; run npm run ipc:generate`);
      }
      console.log(`Current: ${destination}`);
    } else {
      await mkdir(dirname(path), { recursive: true });
      await writeFile(path, text);
      console.log(`Generated: ${destination}`);
    }
  }
} finally {
  await rm(temporary, { recursive: true, force: true });
}
