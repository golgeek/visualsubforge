import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type DocumentSnapshot,
  type DocumentAction,
  type ReviewAction,
  type ReviewReply,
  type ReviewChange,
  type ReviewThread,
} from '../../../src/lib/client';
import { sceneView, waveformView } from '../ipc-contract-samples';
import { emptyQuality } from '../empty-quality';
import { matchesShortcut } from '../../../src/lib/workspace/action-registry';
import { chooseCommand } from '../command-discovery';
const host = document.querySelector<HTMLElement>('#test')!;
let doc: DocumentSnapshot = structuredClone(emptyDocument);
let media = { ...emptyStatus, available: true };
let newer = false;
let exported = false;
let importCase: 'same' | 'new' | 'different' = 'same';
const imports: string[] = [];
let previousCues: typeof doc.cues | null = null;
let starts = 0;
let queryCount = 0;
let discussion: ReviewThread | null = null;
let commentedKey = '';
function discussionPage() {
  const c = rows.find((c) => c.key === commentedKey);
  return {
    rows:
      c && discussion
        ? { [c.key + '/' + c.original[0]!.source_index]: [discussion] }
        : {},
    detached: !c && discussion ? [discussion] : [],
    exported: [],
    blockCounts: {},
    open:
      discussion &&
      ![...discussion.messages]
        .reverse()
        .find((m) => m.type !== 'comment')
        ?.type?.includes('resolve')
        ? 1
        : 0,
  };
}
let epoch = 0;
let savedCopies = 0;
let held: Promise<void> | null = null;
let release: () => void = () => {};
const files: string[] = [];
const history: ReviewChange[][] = [];
const options = {
  tolerance_ms: 1,
  ignore_tags: false,
  ignore_line_breaks: false,
  ignore_whitespace: true,
};
function cue(id: number, text: string, start = 1000) {
  return {
    source_index: id - 1,
    cue_id: id,
    start_ms: start,
    end_ms: start + 2000,
    text,
    reading: text,
    style: null,
    actor: null,
    characters: text.length,
    cps: { status: 'finite' as const, value: 5 },
    reading_speed: { status: 'finite' as const, value: 10 },
  };
}
function change(i: number): ReviewChange {
  return {
    key: `change${i}`,
    kind: i === 1 ? 'timing' : 'text',
    original: [cue(i, 'Original')],
    revised: [cue(i, 'Revised', i === 1 ? 1240 : 1000)],
    words: [
      { kind: 'removed', text: 'Original' },
      { kind: 'added', text: 'Revised' },
    ],
    text_changed: i !== 1,
    timing_changed: i === 1,
    style_changed: false,
  };
}
let rows: ReviewChange[] = Array.from({ length: 201 }, (_, i) => change(i + 1));
rows[100] = {
  ...rows[100]!,
  kind: 'split',
  original: [{ ...cue(101, 'Original', 301000), end_ms: 306000 }],
  revised: [cue(101, 'Part one', 301000), cue(102, 'Part two', 304000)],
};
let blockMode = false;
const blockRow = {
  row: 0,
  block: {
    first: 0,
    end: 3,
    delta_ms: 400,
    start_ms: 1000,
    end_ms: 9400,
    revised_start_ms: 1400,
    revised_end_ms: 9400,
  },
  firstCue: 1,
  lastCue: 3,
};
function displayed(expanded: readonly number[], filter: string | null) {
  const filtered = rows.filter((c) => !filter || c.kind === filter);
  return blockMode && !expanded.includes(0)
    ? filtered.filter((c) => c.key !== 'change2' && c.key !== 'change3')
    : filtered;
}
function reply(): ReviewReply {
  return {
    key: { generation: doc.generation, revision: doc.revision, epoch },
    context: doc.review ?? null,
    discussion: discussionPage(),
    document: null,
    working: false,
    matched: rows.length,
    statistics: {
      original_cues: 201,
      changes: rows.length,
      added_cues: 0,
      text_cues: 200,
      timing_cues: 1,
      style_cues: 0,
      changed_cues: 201,
      text_percent: 99.5,
      timing_percent: 0.5,
      total_percent: 100,
    },
    rows: structuredClone(rows),
    total: rows.length,
    error: null,
    newerOriginal: null,
  };
}
function loaded(path: string) {
  doc = {
    ...emptyDocument,
    loaded: true,
    generation: doc.generation + 1,
    path,
    cues: Array.from({ length: 201 }, (_, i) => ({
      id: i + 1,
      startMs: 1000 + i * 3000,
      endMs: 3000 + i * 3000,
      text: 'Revised',
    })),
  };
}
let menuListener = 0;
const callbacks = new Map<number, (value: unknown) => void>();
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback(callback: (v: unknown) => void) {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback() {},
    async invoke(command: string, args: Record<string, unknown>) {
      switch (command) {
        case 'recent_list':
        case 'recent_capture':
        case 'recent_forget':
          return [];
        case 'plugin:event|listen':
          if (args.event === 'workspace-action')
            menuListener = Number(args.handler);
          return args.handler;
        case 'plugin:event|unlisten':
        case 'update_menu':
        case 'layout_save':
          return;
        case 'preferences_save':
          return 2;
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'quality_read':
          return emptyQuality(args);
        case 'quality_cancel':
          return null;
        case 'quality_settings':
          return {
            current: {
              epoch: 1,
              profile: defaultPreferences.quality,
              timing: defaultPreferences.timing,
            },
            builtins: [defaultPreferences.quality],
          };
        case 'language_words':
          return {
            epoch: 1,
            personal: { englishUs: [], french: [] },
            ignored: { englishUs: [], french: [] },
          };
        case 'recovery_status':
          return { entries: [], error: null };
        case 'choose_file':
          files.push(String(args.kind));
          return args.kind === 'reviewOriginal'
            ? '/original.srt'
            : args.kind === 'reviewRevised'
              ? '/revised.srt'
              : args.kind === 'comparisonReport'
                ? '/report.html'
                : args.kind === 'saveSubtitles'
                  ? String(args.current)
                  : '/media.mp4';
        case 'media_action': {
          const a = args.action as { kind: string };
          if (a.kind === 'open')
            media = {
              ...media,
              loaded: true,
              session: 1,
              durationSeconds: 120,
              nominalFrameRate: 25,
            };
          if (a.kind === 'close') media = { ...emptyStatus, available: true };
          return structuredClone(media);
        }
        case 'document_action': {
          const a = args.action as DocumentAction;
          if (a.kind === 'open') loaded(a.path);
          if (a.kind === 'save') {
            doc.dirty = false;
          }
          if (a.kind === 'edit') {
            doc.revision++;
            doc.dirty = true;
            doc.cues.find((c) => c.id === a.cueId)!.text = a.text;
          }
          if (a.kind === 'undo') {
            if (previousCues) {
              doc.cues = previousCues;
              previousCues = null;
            }
            rows = history.pop() ?? rows;
            doc.revision++;
          }
          return structuredClone(doc);
        }
        case 'review_action': {
          const a = args.action as ReviewAction;
          if (a.kind === 'inspectPackage')
            return {
              ...reply(),
              inspection: {
                case: importCase,
                digest: 'fixture',
                exportedBy: 'Sam',
                exportedAtMs: 21,
              },
            };
          if (a.kind === 'importPackage') {
            imports.push(a.choice);
            if (a.choice === 'open') {
              doc = {
                ...doc,
                generation: doc.generation + 1,
                path: null,
                dirty: true,
              };
            }
            if (a.choice === 'useTheirs') {
              previousCues = structuredClone(doc.cues);
              doc.cues = [{ ...doc.cues[0]!, text: 'Incoming' }];
              doc.revision++;
              doc.canUndo = true;
            }
            doc.review = { ...doc.review!, epoch: ++epoch, round: 2 };
            return {
              ...reply(),
              document: structuredClone(doc),
              notice: '1 new message',
            };
          }
          if (a.kind === 'comment') {
            const edit = a.edit;
            if (edit.kind === 'post') {
              commentedKey = edit.changeKey;
              const c = rows.find((c) => c.key === commentedKey)!;
              discussion = {
                id: 'thread',
                first_message_id: 'first',
                anchor: {
                  kind: 'change',
                  change_key: c.key,
                  change_kind: c.kind,
                  original: c.original,
                  revised: c.revised,
                },
                messages: [
                  {
                    id: 'first',
                    author: a.author,
                    at_ms: a.atMs,
                    type: 'comment',
                    text: edit.text,
                  },
                ],
              };
            } else if (edit.kind === 'reply' && discussion) {
              discussion.messages.push({
                id: 'm' + discussion.messages.length,
                author: a.author,
                at_ms: a.atMs,
                type: edit.event,
                ...(edit.text ? { text: edit.text } : {}),
              });
            }
            doc.review = { ...doc.review!, epoch: ++epoch, dirty: true };
            return reply();
          }
          if (a.kind === 'threads' || a.kind === 'navigateThread') {
            const r = reply();
            r.rows = rows.filter((c) => c.key === commentedKey);
            r.total = r.rows.length + (r.discussion?.detached.length ?? 0);
            r.position = 1;
            r.threadId = discussion?.id ?? null;
            return r;
          }
          if (a.kind === 'inspect') return { ...reply(), newerOriginal: newer };
          if (a.kind === 'attach') {
            doc.review = { epoch: ++epoch, dirty: true, path: a.path, options };
            return { ...reply(), document: structuredClone(doc) };
          }
          if (a.kind === 'start') {
            savedCopies++;
            doc.path = a.path;
            doc.generation++;
            doc.review = {
              epoch: ++epoch,
              dirty: true,
              path: doc.review?.path ?? '/revised.srt',
              options,
            };
            return { ...reply(), document: structuredClone(doc) };
          }
          if (a.kind === 'detach') {
            doc.review = { epoch: ++epoch, dirty: true, path: null, options };
            return { ...reply(), document: structuredClone(doc) };
          }
          if (a.kind === 'options') {
            doc.review = { ...doc.review!, options: a.options, epoch: ++epoch };
            return { ...reply(), document: structuredClone(doc) };
          }
          if (a.kind === 'query') {
            queryCount++;
            starts++;
            const captured = reply();
            if (held) await held;
            const filtered = displayed(a.expanded ?? [], a.filter);
            return {
              ...captured,
              blocks:
                blockMode && a.offset === 0 && filtered[0]?.key === 'change1'
                  ? [blockRow]
                  : [],
              rows: filtered.slice(a.offset, a.offset + a.limit),
              total: filtered.length,
              matched: filtered.length,
            };
          }
          if (a.kind === 'projection')
            return {
              ...reply(),
              rows: rows
                .filter((c) =>
                  c.revised.some((cue) => a.ids.includes(cue.cue_id)),
                )
                .slice(0, 200),
            };
          if (a.kind === 'navigate') {
            const stops = displayed(a.expanded ?? [], a.filter);
            const insideCollapsed =
              blockMode &&
              !(a.expanded ?? []).includes(0) &&
              a.cueId !== null &&
              a.cueId <= 3;
            const index = insideCollapsed
              ? 0
              : stops.findIndex((c) =>
                  a.from
                    ? c.key === a.from
                    : c.revised.some((cue) => cue.cue_id === a.cueId),
                );
            const next = Math.max(
              0,
              Math.min(stops.length - 1, index + (a.forward ? 1 : -1)),
            );
            return {
              ...reply(),
              blocks: blockMode && next === 0 ? [blockRow] : [],
              rows: stops[next] ? [stops[next]!] : [],
              position: next + 1,
            };
          }
          if (a.kind === 'revert') {
            history.push(structuredClone(rows));
            rows = a.changeKey
              ? rows.filter((c) => c.key !== a.changeKey)
              : rows.filter((c) => a.filter && c.kind !== a.filter);
            doc.revision++;
            doc.dirty = true;
            doc.canUndo = true;
            return { ...reply(), document: structuredClone(doc) };
          }
          if (a.kind === 'export') {
            exported = true;
            return reply();
          }
          return reply();
        }
        case 'waveform_view':
          return { ...waveformView, generation: 0, peaks: [] };
        case 'scenes_start':
        case 'scenes_cancel':
        case 'scenes_view':
          return { ...sceneView, generation: 0, cuts: [] };
        default:
          throw new Error('Unexpected command ' + command);
      }
    },
  },
});
function assert(v: unknown, m: string) {
  if (!v) throw new Error(m);
}
function passed(m: string) {
  const li = document.createElement('li');
  li.textContent = 'PASS: ' + m;
  document.querySelector('#checks')!.append(li);
}
function button(text: string) {
  return [...host.querySelectorAll<HTMLButtonElement>('button')].find(
    (b) => b.textContent?.trim() === text,
  )!;
}
async function until(fn: () => boolean) {
  await tick();
  const end = performance.now() + 8000;
  while (!fn()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(
      () => !!button('New subtitles') && !button('New subtitles').disabled,
    );
    await chooseCommand(host, 'compareFiles');
    await until(() => !!host.querySelector('[aria-label="Changes"] article'));
    assert(
      files.slice(0, 2).join(',') === 'reviewOriginal,reviewRevised',
      'Picker roles are distinct',
    );
    assert(doc.path === '/revised.srt', 'Revised is working');
    assert(!media.loaded, 'No media opened');
    passed(
      'Compare Files from an empty workspace opens an editable review without media',
    );
    assert(host.textContent?.includes('+0.240 s'), 'Numeric timing delta');
    assert(
      button('Ignore timing changes under one frame').disabled,
      'Frame option disabled',
    );
    passed('No-media review shows numeric timing and disables frame tolerance');
    button('Next page').click();
    await until(
      () =>
        (host.querySelectorAll('[aria-label="Changes"] article').length ===
          100 &&
          host.textContent?.includes('change') === false) ||
        !!host.querySelector('[aria-label="Changes"] article button'),
    );
    await new Promise((r) => setTimeout(r, 400));
    assert(
      host.querySelectorAll('[aria-label="Changes"] article').length === 100,
      'Second bounded page',
    );
    const groupRow = host.querySelector('[aria-label="Changes"] article')!;
    groupRow.querySelector<HTMLButtonElement>('button')!.click();
    await until(
      () =>
        host.querySelectorAll('[data-cue-id][aria-pressed="true"]').length ===
        2,
    );
    assert(
      host.textContent?.includes('00:00:05,000'),
      'Group range spans both revised cues',
    );
    passed(
      'Split selection includes both working cues and frames the whole group',
    );
    const last = [
      ...host.querySelectorAll('[aria-label="Changes"] article'),
    ].at(-1)!;
    last.querySelector<HTMLButtonElement>('button')!.click();
    await until(
      () => host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    await chooseCommand(host, 'nextChange');
    await until(
      () =>
        host.querySelectorAll('[aria-label="Changes"] article').length === 1,
    );
    assert(
      host.textContent?.includes('Change 201 of 201'),
      'Navigation publishes absolute position',
    );
    passed(
      'Next Change crosses a page boundary and shows its absolute position',
    );
    passed('Changes are paged in bounded 100-row pages');
    button('Timing').click();
    await until(
      () =>
        host.querySelectorAll('[aria-label="Changes"] article').length === 1 &&
        host.textContent?.includes('+0.240 s') === true,
    );
    passed('Timing filter resets paging and keeps the matching row');
    const article = host.querySelector('[aria-label="Changes"] article')!;
    article.querySelector<HTMLButtonElement>('button')!.click();
    await until(
      () =>
        !!host.querySelector('[aria-label="Original"]') &&
        !button('Revert')?.disabled,
    );
    passed('Selecting a change selects its cue and shows the Original diff');
    [...article.querySelectorAll('button')]
      .find((b) => b.textContent === 'Revert')!
      .click();
    await until(
      () =>
        rows.length === 200 &&
        !host.querySelector('[aria-label="Changes"] article'),
    );
    assert(doc.canUndo, 'Revert retained Undo');
    passed('Revert removes a change while retaining ordinary Undo');
    button('All').click();
    await until(
      () =>
        host.querySelectorAll('[aria-label="Changes"] article').length === 100,
    );
    await chooseCommand(host, 'nextChange');
    await until(
      () =>
        !!host.querySelector('[aria-label="Original"]') &&
        !button('Revert')?.disabled,
    );
    passed('Next Change navigates in a subtitles-only review');
    await chooseCommand(host, 'exportComparisonReport');
    await until(() => !!button('Continue to Save'));
    const exportName = host.querySelector<HTMLInputElement>(
      'dialog[open] input[maxlength]',
    )!;
    exportName.value = 'Report reviewer';
    exportName.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    button('Continue to Save').click();
    await until(() => exported);
    passed('Comparison report export works with no media');
    await chooseCommand(host, 'endReview');
    await until(() => !doc.review?.path);
    assert(doc.canUndo, 'End Review preserves history');
    passed('End Review preserves subtitle history');
    // Saving the draft before Start is modeled through the existing protected save flow.
    doc.dirty = false;
    await chooseCommand(host, 'startReview');
    await until(() => !!host.querySelector('dialog[open]') || savedCopies > 0);
    if (button('Discard changes')) button('Discard changes').click();
    await until(() => savedCopies === 1);
    assert(doc.path === '/revised.reviewed.srt', 'Reviewed filename');
    assert(
      host.querySelector('.workspace')?.getAttribute('data-preset') ===
        defaultPreferences.layout.active,
      'Start Review preserves the non-Review preset',
    );
    passed('Start Review writes a reviewed copy and opens Changes');
    newer = true;
    await chooseCommand(host, 'compareOriginal');
    await until(() => !!button('Swap'));
    button('Keep').click();
    await until(() => doc.review?.path === '/original.srt');
    passed('Newer-original prompt supports keeping the current direction');
    held = new Promise<void>((resolve) => (release = resolve));
    button('Timing').click();
    await until(() => starts > 0);
    button('All').click();
    held = null;
    release();
    await until(
      () =>
        host.querySelectorAll('[aria-label="Changes"] article').length === 100,
    );
    passed('Latest filter wins after a delayed comparison reply');
    held = new Promise<void>((resolve) => (release = resolve));
    const beforeHeld = starts;
    button('Timing').click();
    await until(() => starts > beforeHeld);
    const editor = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Subtitle text"]',
    )!;
    editor.value = 'Typed while comparison is pending';
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    await chooseCommand(host, 'apply');
    await until(() => doc.dirty);
    held = null;
    release();
    await until(
      () =>
        host.textContent
          ?.replace(/\s+/gu, ' ')
          .includes('Revert 0 Timing Changes') === true,
    );
    button('All').click();
    await until(() => !!host.querySelector('[aria-label="Changes"] article'));
    assert(
      editor.value === 'Typed while comparison is pending',
      'Old result cannot replace edited text',
    );
    passed(
      'Typing and committing while a comparison is pending rejects the old revision',
    );
    await until(
      () => host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    await chooseCommand(host, 'compareOriginal');
    await until(() => !!button('Swap'));
    button('Swap').click();
    await until(
      () =>
        button('Discard changes')?.closest('dialog')?.open === true &&
        !button('Discard changes').disabled,
    );
    assert(
      doc.path === '/revised.reviewed.srt',
      'Swap protects current unsaved work before opening another document',
    );
    button('Discard changes').click();
    await until(
      () =>
        doc.path === '/original.srt' &&
        doc.review?.path === '/revised.reviewed.srt',
    );
    passed(
      'Swap goes through unsaved protection and reverses the working and original files',
    );
    await until(
      () =>
        host.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !!host.querySelector('[aria-label="Changes"] article'),
    );
    const beforeMedia = queryCount;
    await chooseCommand(host, 'openMedia');
    await until(
      () => media.loaded && !!host.querySelector('[data-preview-cue]'),
    );
    assert(
      queryCount === beforeMedia,
      'Opening media does not request a comparison',
    );
    passed(
      'Opening media mid-review adds original timing ghosts without recomputing',
    );
    blockMode = true;
    rows = Array.from({ length: 201 }, (_, i) => change(i + 1));
    for (const c of rows.slice(0, 3)) c.kind = 'timing';
    button('Timing').click();
    await until(() => !!button('Expand'));
    assert(
      host.querySelectorAll('[aria-label="Changes"] article').length === 1,
      'Collapsed block occupies one stop',
    );
    host
      .querySelector<HTMLButtonElement>(
        '[aria-label="Changes"] article button',
      )!
      .click();
    await until(
      () =>
        host.querySelectorAll('[data-cue-id][aria-pressed="true"]').length ===
        3,
    );
    button('Expand').click();
    await until(
      () =>
        host.querySelectorAll('[aria-label="Changes"] article').length === 3,
    );
    button('Next Change').click();
    await until(() => host.textContent?.includes('Change 2 of 3') ?? false);
    assert(
      !!button('Shifted 3 cues by +0.400 s (00:00:01,400 – 00:00:09,400)'),
      'Block label uses the revised timeline',
    );
    button('Collapse').click();
    await until(() => !!button('Expand'));
    button('All').click();
    await until(() => host.textContent?.includes('of 199') ?? false);
    button('Next Change').click();
    await until(() => host.textContent?.includes('Change 2 of 199') ?? false);
    host.querySelector<HTMLElement>('[data-cue-id="9"]')!.click();
    await until(
      () => host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    await until(
      () => !!host.querySelector('[data-cue-id="9"][aria-pressed="true"]'),
    );
    button('Previous Change').click();
    await until(() => host.textContent?.includes('Change 6 of 199') ?? false);
    passed(
      'Block selection, collapsed paging, expanded navigation and Previous Change after selecting another cue',
    );
    const changes = host.querySelector('section[aria-label="Changes"]')!;
    const inPanel = (label: string) =>
      [...changes.querySelectorAll('button')].find(
        (b) => b.textContent?.trim() === label,
      );
    const fill = (selector: string, value: string) => {
      const input = changes.querySelector<
        HTMLInputElement | HTMLTextAreaElement
      >(selector)!;
      input.value = value;
      input.dispatchEvent(new Event('input', { bubbles: true }));
    };
    inPanel('Comment')!.click();
    await tick();
    assert(
      changes.textContent?.includes(
        'Shown to everyone who receives your review files',
      ),
      'Inline name disclosure',
    );
    fill('form input', 'Ana');
    fill('form textarea', '<img> Check');
    await tick();
    inPanel('Post')!.click();
    await until(() => !!inPanel('Reply'));
    assert(
      changes.textContent?.includes('<img> Check') &&
        !changes.querySelector('img'),
      'Literal comment',
    );
    inPanel('Reply')!.click();
    await tick();
    fill('form textarea', 'Agreed');
    await tick();
    inPanel('Post')!.click();
    await until(
      () => !!inPanel('Resolve') && !!changes.textContent?.includes('Agreed'),
    );
    inPanel('Resolve')!.click();
    await until(() => !!inPanel('Reopen'));
    inPanel('Reopen')!.click();
    await until(() => !!inPanel('Resolve'));
    inPanel('Commented')!.click();
    await until(() => changes.querySelectorAll('article').length === 1);
    await chooseCommand(host, 'nextOpenThread');
    await until(
      () => inPanel('Open threads')?.getAttribute('aria-pressed') === 'true',
    );
    await until(() => !!inPanel('Revert') && !inPanel('Revert')!.disabled);
    inPanel('Revert')!.click();
    await until(() => !!changes.querySelector('[aria-label="Detached"]'));
    inPanel('Reply')!.click();
    await tick();
    fill('form textarea', 'Still visible');
    await tick();
    inPanel('Post')!.click();
    await until(() => !!changes.textContent?.includes('Still visible'));
    passed(
      'Name prompt, literal comments, replies, status events, comment filters, navigation and detached replies',
    );
    const exchangeCommand = async (
      id: 'openReviewPackage' | 'exportReviewPackage',
    ) => {
      await until(
        () =>
          !host.querySelector('dialog[open]') &&
          host.querySelector('main')?.getAttribute('aria-busy') === 'false',
      );
      await chooseCommand(host, id);
    };
    await exchangeCommand('exportReviewPackage');
    await until(() => !!button('Continue to Save'));
    assert(
      host
        .querySelector('dialog[open]')
        ?.textContent?.includes('The package includes your unsaved changes'),
      'Unsaved package disclosure',
    );
    [...host.querySelectorAll<HTMLButtonElement>('dialog[open] button')]
      .find((b) => b.textContent?.trim() === 'Cancel')!
      .click();
    await exchangeCommand('openReviewPackage');
    await until(() => imports.includes('merge'));
    importCase = 'different';
    await exchangeCommand('openReviewPackage');
    await until(() => !!button('Keep Mine'));
    button('Keep Mine')!.click();
    await until(() => imports.includes('keep'));
    await exchangeCommand('openReviewPackage');
    await until(() => !!button('Use Their Version'));
    [...host.querySelectorAll<HTMLButtonElement>('dialog[open] button')]
      .find((b) => b.textContent?.trim() === 'Cancel')!
      .click();
    assert(imports.length === 2, 'Cancel does not adopt');
    const before = doc.cues[0]!.text;
    await exchangeCommand('openReviewPackage');
    await until(() => !!button('Use Their Version'));
    button('Use Their Version')!.click();
    await until(() => doc.cues[0]?.text === 'Incoming');
    await until(
      () =>
        !button('Undo')?.disabled &&
        host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    // Model the macOS menu bridge from the configured global accelerator to its Tauri event.
    const shortcut = new KeyboardEvent('keydown', {
      key: 'z',
      code: 'KeyZ',
      metaKey: true,
      bubbles: true,
    });
    const binding = defaultPreferences.shortcuts.bindings.find(
      (b) => b.mode === 'global' && matchesShortcut(b.key, shortcut),
    );
    assert(binding?.action === 'undo', 'Command-Z is bound to Undo');
    callbacks.get(menuListener)!({ payload: binding!.action });
    await until(() => doc.cues[0]?.text === before);
    importCase = 'new';
    await exchangeCommand('openReviewPackage');
    await until(() => !!button('Discard changes'));
    button('Discard changes')!.click();
    await until(() => imports.includes('open') && doc.path === null);
    passed(
      'Export disclosure, new and same package opens, different-version choices and one-step Undo',
    );
    assert(queryCount > 0, 'Comparison exercised');
    document.querySelector('#result')!.textContent =
      `PASS: all ${document.querySelectorAll('#checks li').length} version comparison checks`;
  } catch (e) {
    document.querySelector('#result')!.textContent = 'FAIL: ' + String(e);
    console.error(e);
  } finally {
    await unmount(app);
  }
}
void run();
