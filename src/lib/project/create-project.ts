import type { CreateProjectOutcome, DocumentSnapshot } from '../client';

export interface ProjectNames {
  projectName: string;
  folder: string;
  companionName: string | null;
}

export interface CreateProjectPrompt extends ProjectNames {
  reference: string | null;
  suffix: string;
  submit: (names: ProjectNames) => Promise<CreateProjectOutcome>;
}

export function projectDefaults(doc: DocumentSnapshot, mediaPath: string) {
  const source = doc.suggestedSavePath ?? doc.path ?? mediaPath;
  const split = Math.max(source.lastIndexOf('/'), source.lastIndexOf('\\')) + 1;
  const folder = source.slice(0, split);
  const stem = source.slice(split).replace(/\.[^.]+$/, '') || 'Untitled';
  const language = /\.[a-z]{2,3}(?:-[a-z0-9]{2,8})*$/i.exec(stem)?.[0] ?? '';
  const projectName = stem.slice(0, stem.length - language.length) || stem;
  const suffix = language + '.' + (doc.format?.name ?? 'srt');
  const needsCompanion = doc.loaded && (!doc.path || doc.requiresSaveAs);
  return {
    projectName,
    folder,
    suffix,
    companionName: needsCompanion ? projectName + suffix : null,
    reference: needsCompanion ? null : doc.path,
  };
}
