import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import type { Plugin } from 'vite';

interface PackageInfo {
  name: string;
  version: string;
  license: string;
  path: string;
  role: 'bundled-module' | 'generated-runtime';
  notices: { path: string; sha256: string; text: string }[];
}

/** Build evidence only. It never enters the editor's state or IPC boundary. */
export function frontendInventory(): Plugin {
  let root = '';
  const relative = (file: string) =>
    path.relative(root, file).split(path.sep).join('/');
  const hash = (bytes: Buffer | string) =>
    createHash('sha256').update(bytes).digest('hex');

  function packageInfo(
    directory: string,
    role: PackageInfo['role'],
  ): PackageInfo {
    const manifest = JSON.parse(
      readFileSync(path.join(directory, 'package.json'), 'utf8'),
    ) as {
      name: string;
      version: string;
      license: string;
    };
    if (!manifest.name || !manifest.version || !manifest.license) {
      throw new Error(
        `Missing package identity/license: ${relative(directory)}`,
      );
    }
    const notices = readdirSync(directory)
      .filter((name) => /^(license|copying|notice)([.-]|$)/i.test(name))
      .sort()
      .map((name) => {
        const text = readFileSync(path.join(directory, name), 'utf8');
        return { path: name, sha256: hash(text), text };
      });
    if (!notices.length)
      throw new Error(`Missing package notices: ${manifest.name}`);
    return {
      name: manifest.name,
      version: manifest.version,
      license: manifest.license,
      path: relative(directory),
      role,
      notices,
    };
  }

  function owner(file: string): string | undefined {
    let directory = path.dirname(file);
    while (directory !== root && directory !== path.dirname(directory)) {
      if (existsSync(path.join(directory, 'package.json'))) return directory;
      directory = path.dirname(directory);
    }
    return undefined;
  }

  return {
    name: 'vsf-frontend-inventory',
    apply: 'build',
    configResolved(config) {
      root = config.root;
    },
    generateBundle(_options, bundle) {
      const packages = new Map<string, PackageInfo>();
      // These tools can inject helpers represented by virtual module IDs. Retain
      // their published source/notices even when no physical module ID appears.
      for (const name of ['vite', 'rolldown']) {
        const info = packageInfo(
          path.join(root, 'node_modules', name),
          'generated-runtime',
        );
        packages.set(info.path, info);
      }
      const chunks = [];
      for (const item of Object.values(bundle)) {
        if (item.type !== 'chunk') continue;
        const modules = [];
        for (const id of Object.keys(item.modules).sort()) {
          if (id.startsWith('\0')) {
            modules.push({
              id: id.replaceAll(root, '<project>'),
              virtual: true,
            });
            continue;
          }
          const file = id.split('?')[0];
          if (!file || !path.isAbsolute(file) || !existsSync(file)) {
            throw new Error(`Unidentified bundled module: ${id}`);
          }
          const name = relative(file);
          if (name.startsWith('../'))
            throw new Error(`Bundled module escapes project: ${name}`);
          modules.push({ id: name, sha256: hash(readFileSync(file)) });
          if (name.includes('node_modules/')) {
            const directory = owner(file);
            if (!directory) throw new Error(`No package owns ${name}`);
            const info = packageInfo(directory, 'bundled-module');
            packages.set(info.path, info);
          }
        }
        chunks.push({ file: item.fileName, modules });
      }
      const inventory = {
        schema: 1,
        scope:
          'Modules contributing to production chunks, plus generators of embedded helpers',
        packages: [...packages.values()].sort((a, b) =>
          a.name.localeCompare(b.name),
        ),
        chunks,
      };
      this.emitFile({
        type: 'asset',
        fileName: 'frontend-inventory.json',
        source: JSON.stringify(inventory, null, 2) + '\n',
      });
      this.emitFile({
        type: 'asset',
        fileName: 'THIRD-PARTY-NOTICES.txt',
        source: inventory.packages
          .map(
            (entry) =>
              `${entry.name} ${entry.version}\n${entry.notices.map((notice) => notice.text).join('\n')}\n`,
          )
          .join('\n'),
      });
    },
  };
}
