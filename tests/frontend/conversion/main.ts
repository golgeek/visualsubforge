import { mount, unmount, tick } from 'svelte';
import ConversionDialog from '../../../src/lib/ConversionDialog.svelte';
import { ConversionSession } from '../../../src/lib/conversion-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import {
  emptyDocument,
  type ConversionOptions,
  type ConversionReview,
  type DocumentSnapshot,
  type FileChoice,
} from '../../../src/lib/client';
import {
  conversionReview,
  conversionBlocked,
  styledDocument,
} from '../ipc-contract-samples';

const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
function select(label: string, value: string) {
  const field = host.querySelector<HTMLSelectElement>(
    `[aria-label="${label}"]`,
  )!;
  field.value = value;
  field.dispatchEvent(new Event('change', { bubbles: true }));
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, `Timed out: ${host.textContent}`);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function snapshot(): DocumentSnapshot {
  return {
    ...structuredClone(emptyDocument),
    loaded: true,
    generation: 1,
    revision: 2,
    path: '/fixture/source.srt',
    dirty: true,
    canUndo: true,
    canRedo: true,
    cues: [
      { id: 1, startMs: 1001, endMs: 2001, text: '{\\k20}世界 <b>hello</b>' },
    ],
  };
}
function review(
  options: ConversionOptions,
  doc: DocumentSnapshot,
  blocked = false,
): ConversionReview {
  const result: ConversionReview = structuredClone(
    blocked ? conversionBlocked : conversionReview,
  );
  result.options = { ...options };
  result.report!.options = { ...options };
  result.report!.source = doc.format
    ? (doc.format.name.toLowerCase() as 'ass' | 'ssa')
    : 'srt';
  result.key.generation = doc.generation;
  result.key.revision = doc.revision;
  return result;
}

let doc = snapshot(),
  view: ConversionReview = structuredClone(conversionReview);
let requested = 0,
  published = 0,
  cancelled = 0,
  pickerCancel = false,
  failPublish = false,
  holdPreparation = false;
let closePath: string | undefined;
let closed = false;
let chosenKind: FileChoice | null = null;
let suggested: string | null = null;
let failApply = false;
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: {
        options?: ConversionOptions;
        kind?: FileChoice;
        current?: string;
        path?: string;
      },
    ) => {
      switch (command) {
        case 'conversion_start': {
          requested++;
          view = review(
            args.options!,
            doc,
            !doc.format && args.options!.precision === 'exact',
          );
          view.id = requested;
          if (holdPreparation) {
            view.phase = 'preparing';
            view.report = null;
          } else if (view.report) {
            view.report.issues.push({
              code: 'fixture_text',
              severity: 'loss',
              message: 'Style <b>opaque</b> is not portable',
              count: 10000,
              examples: [
                {
                  cueId: '1',
                  detail: '<img src=x onerror=alert(1)> {\\pos(5,6)}',
                },
              ],
            });
          }
          return requested;
        }
        case 'conversion_view':
          return structuredClone(view);
        case 'conversion_cancel':
          cancelled++;
          if (view.phase !== 'published')
            view = { ...view, phase: 'cancelled' };
          return;
        case 'choose_file':
          chosenKind = args.kind!;
          suggested = args.current!;
          return pickerCancel ? null : `/fixture/export.${view.options.target}`;
        case 'conversion_publish':
          published++;
          if (failPublish) {
            view = {
              ...view,
              phase: 'failed',
              error: 'Output already exists; choose a new filename',
            };
            return;
          }
          view = {
            ...view,
            phase: 'published',
            error: null,
            outputPath: args.path!,
          };
          return;
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
const subtitles = new DocumentSession({
  chooseFile: async () => null,
  resyncAccept: async () => {
    throw new Error('Export cannot adopt a document');
  },
  gapAccept: async () => {
    throw new Error('Export cannot adopt a document');
  },
  documentAction: async (action) => {
    if (action.kind === 'edit') {
      if (failApply) throw new Error('Draft validation failed');
      doc = {
        ...doc,
        revision: doc.revision + 1,
        cues: [{ ...doc.cues[0]!, text: action.text }],
      };
    } else
      assert(
        action.kind === 'status',
        `Unexpected document mutation ${action.kind}`,
      );
    return structuredClone(doc);
  },
});
let app: ReturnType<typeof mount> | null = null;
async function show() {
  closed = false;
  app = mount(ConversionDialog, {
    target: host,
    props: {
      subtitles,
      onclose: (path) => {
        closed = true;
        closePath = path;
      },
    },
  });
  await until(() => !!host.querySelector('dialog[open]'));
}
async function hide() {
  await unmount(app!);
  app = null;
}

// These injected clients hold exact asynchronous boundaries. The report shape
// comes from real Serde samples; format/IO correctness belongs to Rust/native tests.
async function harness() {
  const state = {
    document: snapshot(),
    view: structuredClone(conversionReview) as ConversionReview,
    cancels: [] as number[],
    publications: [] as string[],
    reads: 0,
    choose: null as null | (() => Promise<string | null>),
    start: null as null | (() => Promise<number>),
    read: null as null | (() => Promise<ConversionReview>),
    publish: null as null | (() => Promise<void>),
    cancel: null as null | (() => Promise<void>),
    choices: [] as { kind: FileChoice; current?: string | null }[],
  };
  const owner = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async (action) => {
      assert(action.kind === 'status', 'Conversion changed accepted state');
      return state.document;
    },
    resyncAccept: async () => {
      throw new Error('Unexpected adoption');
    },
    gapAccept: async () => {
      throw new Error('Unexpected adoption');
    },
  });
  await owner.command({ kind: 'status' });
  const session = new ConversionSession(owner, {
    chooseFile: async (kind, current = null) => {
      state.choices.push({ kind, current });
      return state.choose ? state.choose() : '/new.ass';
    },
    conversionStart: async (_, options) => {
      state.view = review(options, state.document);
      return state.start ? state.start() : 41;
    },
    conversionView: async () => {
      state.reads++;
      return state.read ? state.read() : structuredClone(state.view);
    },
    conversionCancel: async (id) => {
      state.cancels.push(id);
      if (state.cancel) await state.cancel();
    },
    conversionPublish: async (_, path) => {
      state.publications.push(path);
      if (state.publish) await state.publish();
      else state.view = { ...state.view, phase: 'published', outputPath: path };
    },
  });
  return {
    state,
    owner,
    session,
    dispose: () => {
      session.dispose();
      owner.dispose();
    },
  };
}

async function lifecycle() {
  {
    const h = await harness();
    const late = deferred<number>();
    h.state.start = () => late.promise;
    const starting = h.session.start();
    h.session.dispose();
    late.resolve(777);
    await starting;
    assert(
      h.state.cancels.includes(777) && !h.session.view,
      'Disposed dialog adopted a late job',
    );
    h.dispose();
    passed('Unmount cancels a job that arrives after the dialog has closed');
  }
  {
    const h = await harness();
    await h.session.start();
    const late = deferred<ConversionReview>();
    h.state.read = () => late.promise;
    const polling = h.session.poll();
    const reads = h.state.reads;
    await h.session.poll();
    assert(h.state.reads === reads, 'Concurrent status polls reached IPC');
    h.owner.select(1);
    h.owner.text = 'unapplied';
    assert(!h.session.canPublish, 'Cue draft left export enabled');
    late.resolve(h.state.view);
    await polling;
    assert(
      !h.session.view && h.state.cancels.includes(41),
      'Late view survived an unapplied draft',
    );
    h.dispose();
    passed(
      'One status read runs at a time; cue drafts immediately block and retire stale reviews',
    );
  }
  {
    const h = await harness();
    await h.session.start();
    const late = deferred<ConversionReview>();
    h.state.read = () => late.promise;
    const polling = h.session.poll();
    h.session.configure({ target: 'ssa', precision: 'nearest' });
    late.resolve(h.state.view);
    await polling;
    assert(
      !h.session.view && !h.session.canPublish && h.state.cancels.includes(41),
      'Old options revived a retired report',
    );
    h.state.read = null;
    await h.session.start();
    h.state.document = { ...h.state.document, path: '/renamed.srt' };
    await h.owner.command({ kind: 'status' });
    assert(
      !h.session.canPublish,
      'Save As with unchanged revision left export enabled',
    );
    await h.session.poll();
    assert(!h.session.view, 'Changed source identity retained a report');
    h.dispose();
    passed(
      'Option changes and Save As invalidate review even when a reply or revision is unchanged',
    );
  }
  for (const change of ['document', 'draft', 'dispose'] as const) {
    const h = await harness();
    await h.session.start();
    const picker = deferred<string | null>();
    h.state.choose = () => picker.promise;
    const publishing = h.session.publish();
    assert(
      !h.session.canClose && !h.session.canStart,
      'Picker did not protect dialog lifetime',
    );
    if (change === 'document') {
      h.state.document = {
        ...h.state.document,
        revision: h.state.document.revision + 1,
      };
      await h.owner.command({ kind: 'status' });
    } else if (change === 'draft') {
      h.owner.select(1);
      h.owner.text = 'draft';
    } else h.session.dispose();
    picker.resolve('/new.ass');
    await publishing;
    assert(
      h.state.publications.length === 0,
      `Late picker exported after ${change}`,
    );
    if (change !== 'dispose')
      assert(
        h.session.error.includes('changed while choosing'),
        'Missing stale picker diagnostic',
      );
    h.dispose();
  }
  passed(
    'A late file picker cannot export after a document change, cue draft or unmount',
  );
  {
    const h = await harness();
    await h.session.start();
    h.state.publish = async () => {
      throw new Error('Dispatch rejected');
    };
    await h.session.publish();
    assert(
      h.session.canClose &&
        h.session.canPublish &&
        h.session.error.includes('Dispatch rejected'),
      'Confirmed dispatch rejection was not retryable',
    );
    h.dispose();
    passed(
      'A rejected dispatch with an authoritative review status allows safe retry',
    );
  }
  {
    const h = await harness();
    const original = h.owner.snapshot;
    await h.session.start();
    h.state.publish = async () => {
      throw new Error('Dispatch reply lost');
    };
    h.state.read = async () => {
      throw new Error('Status unavailable');
    };
    await h.session.publish();
    assert(
      h.session.running &&
        !h.session.canClose &&
        !h.session.canConfigure &&
        !h.session.canPublish,
      'Unknown publication outcome allowed duplicate export or close',
    );
    const options = h.session.options;
    h.session.configure({ target: 'ssa', precision: 'nearest' });
    assert(
      h.session.options === options,
      'Unknown publication outcome allowed changed options',
    );
    await h.session.cancel();
    assert(
      h.state.cancels.includes(41) && !h.session.canClose,
      'Uncertain publication could not be cancelled or lost its lifetime guard',
    );
    h.state.read = null;
    h.state.view = {
      ...h.state.view,
      phase: 'published',
      outputPath: '/completed.ass',
    };
    await h.session.poll();
    assert(
      h.session.canClose &&
        h.session.view?.outputPath === '/completed.ass' &&
        h.owner.snapshot === original,
      'Late receipt was lost or replaced the original document',
    );
    h.dispose();
    passed(
      'Lost export replies retain cancellation and lifetime guards until the completed receipt arrives',
    );
  }
  {
    const h = await harness();
    await h.session.start();
    h.state.publish = async () => {
      h.state.view = { ...h.state.view, phase: 'publishing' };
    };
    await h.session.publish();
    assert(
      !h.session.canClose && !h.session.canPublish,
      'Export dispatch was treated as completion',
    );
    h.state.document = {
      ...h.state.document,
      revision: 12,
      path: '/changed.srt',
    };
    await h.owner.command({ kind: 'status' });
    await h.session.poll();
    assert(
      h.state.cancels.includes(41) && !h.session.canClose,
      'Changed source did not cancel an active export',
    );
    h.state.view = {
      ...h.state.view,
      phase: 'published',
      outputPath: '/won-race.ass',
    };
    await h.session.cancel();
    assert(
      h.session.canClose &&
        h.session.view?.phase === 'published' &&
        h.session.view.outputPath === '/won-race.ass',
      'Late cancel hid a successful publication',
    );
    assert(
      h.owner.snapshot === h.state.document && h.owner.snapshot.revision === 12,
      'Publication reverted the changed owner projection',
    );
    h.dispose();
    passed(
      'Cancellation and source changes preserve a successful receipt when publication wins the race',
    );
  }
  for (const [path, target, expected] of [
    ['/with.dots/name.srt', 'ass', '/with.dots/name.converted.ass'],
    ['C:\\with.dots\\name.srt', 'ssa', 'C:\\with.dots\\name.converted.ssa'],
    ['/home/.subtitles', 'srt', '/home/.subtitles.converted.srt'],
  ] as const) {
    const h = await harness();
    h.state.document = {
      ...h.state.document,
      path,
      format: target === 'srt' ? structuredClone(styledDocument.format) : null,
    };
    await h.owner.command({ kind: 'status' });
    h.session.configure({ target, precision: 'nearest' });
    if (target === 'srt')
      assert(
        h.session.options.precision === 'exact',
        'SRT retained a meaningless rounding policy',
      );
    await h.session.start();
    h.state.choose = async () => null;
    await h.session.publish();
    const choice = h.state.choices[0]!;
    assert(
      choice.current === expected &&
        choice.kind.toLowerCase() === `export${target}`,
      'Incorrect destination filter or suggestion',
    );
    assert(
      h.state.publications.length === 0 && h.session.canPublish,
      'Picker cancellation retired a valid review',
    );
    h.dispose();
  }
  passed(
    'All three destination filters suggest separate filenames across POSIX, Windows and hidden paths',
  );
}

async function run() {
  try {
    await subtitles.command({ kind: 'status' });
    subtitles.select(1);
    subtitles.text = 'unapplied';
    await show();
    assert(
      button('Review conversion').disabled,
      'Cue draft did not block review',
    );
    failApply = true;
    button('Apply cue draft').click();
    await until(() => host.textContent!.includes('Draft validation failed'));
    assert(
      subtitles.dirty && button('Review conversion').disabled,
      'Failed Apply discarded the draft',
    );
    button('Discard cue draft').click();
    await tick();
    assert(
      !subtitles.dirty && !button('Review conversion').disabled,
      'Discard did not restore conversion',
    );
    assert(
      host.querySelector<HTMLSelectElement>(
        '[aria-label="Conversion precision"]',
      )!.value === 'exact',
      'Timing rounding was enabled by default',
    );
    button('Review conversion').click();
    await until(() => host.textContent!.includes('Export is blocked'));
    assert(
      button('Choose new output file…').disabled && published === 0,
      'Blocked report allowed publication',
    );
    passed(
      'Drafts require resolution; exact timing is default and blocked reports cannot export',
    );
    select('Conversion precision', 'nearest');
    await tick();
    assert(
      !host.querySelector('[aria-label="Conversion loss report"]') &&
        cancelled > 0,
      'Changed timing policy retained old loss report',
    );
    button('Review conversion').click();
    await until(
      () => !!host.querySelector('[aria-label="Conversion loss report"]'),
    );
    const original = subtitles.snapshot;
    assert(
      host.textContent!.includes('Fansub ASS/SSA overrides are retained') &&
        /at most eight\s+examples/.test(host.textContent!),
      'Compatibility or report bounds missing',
    );
    assert(
      host.textContent!.includes('<img src=x onerror=alert(1)>') &&
        !host.querySelector('img, b'),
      'Report text was interpreted as markup',
    );
    passed(
      'Nearest timing requires a new review; fansub compatibility, exact counts and escaped examples are visible',
    );
    pickerCancel = true;
    button('Choose new output file…').click();
    await until(() => !button('Choose new output file…').disabled);
    assert(
      published === 0 &&
        chosenKind === 'exportAss' &&
        suggested === '/fixture/source.converted.ass',
      'Cancelled picker published or used wrong destination',
    );
    pickerCancel = false;
    failPublish = true;
    button('Choose new output file…').click();
    await until(() => host.textContent!.includes('Output already exists'));
    assert(
      !button('Choose new output file…').disabled &&
        subtitles.snapshot === original,
      'Failed export lost the review or mutated the document',
    );
    failPublish = false;
    button('Choose new output file…').click();
    await until(
      () => !!host.querySelector('[aria-label="Conversion exported"]'),
    );
    assert(
      subtitles.snapshot === original &&
        subtitles.snapshot.canRedo &&
        subtitles.snapshot.dirty,
      'Export changed document history or saved state',
    );
    button('Close conversion').click();
    assert(
      closed && closePath === '/fixture/export.ass',
      'Close lost the publication receipt',
    );
    await hide();
    passed(
      'Picker cancellation and failed export preserve review; successful export preserves the original state and returns its path',
    );
    failApply = false;
    subtitles.text = 'accepted draft';
    await show();
    button('Apply cue draft').click();
    await until(() => !button('Review conversion').disabled);
    assert(
      !subtitles.dirty && subtitles.snapshot.cues[0]!.text === 'accepted draft',
      'Explicit Apply did not reconcile through document owner',
    );
    select('Conversion format', 'ssa');
    holdPreparation = true;
    button('Review conversion').click();
    await until(() => !button('Cancel conversion').disabled);
    button('Cancel conversion').click();
    await until(() => host.textContent!.includes('Conversion cancelled'));
    assert(
      !button('Review conversion').disabled && view.options.target === 'ssa',
      'Cancelled preparation cannot be restarted',
    );
    await hide();
    holdPreparation = false;
    passed(
      'Explicit draft Apply uses the document owner, and in-progress preparation can be cancelled and restarted',
    );
    doc = {
      ...doc,
      format: structuredClone(styledDocument.format),
      path: '/fixture/source.ass',
    };
    await subtitles.command({ kind: 'status' });
    await show();
    assert(
      host.querySelector<HTMLSelectElement>('[aria-label="Conversion format"]')!
        .value === 'srt',
      'Styled source did not default to SRT output',
    );
    assert(
      host.querySelector<HTMLSelectElement>(
        '[aria-label="Conversion precision"]',
      )!.disabled,
      'SRT output offered unnecessary rounding',
    );
    button('Review conversion').click();
    await until(
      () => !!host.querySelector('[aria-label="Conversion loss report"]'),
    );
    pickerCancel = true;
    button('Choose new output file…').click();
    await until(() => !button('Choose new output file…').disabled);
    assert(
      chosenKind === 'exportSrt',
      'Styled source used same-format Save As',
    );
    view = { ...view, phase: 'stale' };
    await until(() => host.textContent!.includes('review is out of date'));
    assert(
      button('Choose new output file…').disabled,
      'Stale report remained publishable',
    );
    await hide();
    passed(
      'Styled sources offer SRT conversion with explicit loss reporting and stale publication refusal',
    );
    await lifecycle();
    heading.textContent = `PASS: all ${output.querySelectorAll('li').length} conversion checks`;
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
    console.error(error);
  } finally {
    if (app) await unmount(app);
    subtitles.dispose();
  }
}
void run();
