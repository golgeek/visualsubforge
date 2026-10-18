import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import {
  defaultPreferences,
  emptyStatus,
  type DocumentSnapshot,
  type DocumentAction,
  type MediaAction,
  type TextFormattingContext,
} from '../../../src/lib/client';
import { styledDocument } from '../ipc-contract-samples';

const host = document.querySelector<HTMLElement>('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const original: DocumentSnapshot = {
  ...structuredClone(styledDocument),
  path: '/fixture.ass',
  revision: 0,
  dirty: false,
  requiresSaveAs: false,
  canUndo: false,
};
let doc = structuredClone(original),
  before = structuredClone(original),
  after = structuredClone(original);
const edits: DocumentAction[] = [];
let fontNotices = ['Font missing: <b>VSF Test Font</b>; fallback may be used'];
let occluded = false;
let eventHandler = 0;
const callbacks = new Map<number, (value: unknown) => void>();
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(text: string) {
  const button = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!button) throw new Error(`Missing ${text}`);
  return button;
}
function field(label: string) {
  const input = host.querySelector<
    HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement
  >(`[aria-label="${label}"]`);
  if (!input) throw new Error(`Missing ${label}`);
  return input;
}
function input(label: string, value: string) {
  const element = field(label);
  element.value = value;
  element.dispatchEvent(
    new Event(element instanceof HTMLSelectElement ? 'change' : 'input', {
      bubbles: true,
    }),
  );
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}

Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback: (callback: (value: unknown) => void) => {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: () => {},
    invoke: async (
      command: string,
      args: {
        action?: DocumentAction | MediaAction;
        handler?: number;
        context?: TextFormattingContext;
        text?: string;
        start?: number;
        end?: number;
      },
    ) => {
      switch (command) {
        case 'plugin:event|listen':
          eventHandler = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if (args.action?.kind === 'occlude') occluded = args.action.occluded;
          return {
            ...structuredClone(emptyStatus),
            available: true,
            surfaceAboveWebview: true,
            subtitleFontDiagnostics: [...fontNotices],
          };
        case 'quality_read':
          throw new Error('Quality disabled in this fixture');
        case 'choose_file':
          return null;
        case 'text_format': {
          assert(
            args.context?.generation === doc.generation &&
              args.context.revision === doc.revision &&
              args.context.cueId === doc.cues[0]!.id &&
              args.context.styleName === 'Title',
            'Formatting did not capture document identity and the draft style',
          );
          assert(args.start === 0 && args.end === 6, 'Wrong selection');
          return {
            text: `{\\b0}${args.text!.slice(0, 6)}{\\b1}${args.text!.slice(6)}`,
            start: 5,
            end: 11,
          };
        }
        case 'document_action': {
          const action = args.action! as DocumentAction;
          if (action.kind === 'editStyled') {
            if (action.startMs % 10 || action.endMs % 10)
              throw new Error('ASS/SSA cue times must use 10 ms increments');
            edits.push(action);
            before = structuredClone(doc);
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              canUndo: true,
              canRedo: false,
              cues: doc.cues.map((c) =>
                c.id === action.cueId
                  ? {
                      ...c,
                      startMs: action.startMs,
                      endMs: action.endMs,
                      text: action.text.replaceAll('\n', '\\N'),
                      style: action.style,
                    }
                  : c,
              ),
            };
            after = structuredClone(doc);
          } else if (action.kind === 'undo')
            doc = { ...before, revision: doc.revision + 1, canRedo: true };
          else if (action.kind === 'redo')
            doc = { ...after, revision: doc.revision + 1 };
          else
            assert(
              action.kind === 'status',
              `Unexpected action ${action.kind}`,
            );
          return structuredClone(doc);
        }
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
const app = mount(App, { target: host });
async function run() {
  try {
    await until(() => host.querySelectorAll('.cue-row').length === 2);
    await until(
      () => !!host.querySelector('[aria-label="Subtitle font notices"]'),
    );
    const notices = host.querySelector('[aria-label="Subtitle font notices"]')!;
    assert(
      notices.textContent?.includes(fontNotices[0]!) &&
        !notices.querySelector('b'),
      'Font notices must be visible as escaped text',
    );
    assert(
      !doc.dirty && edits.length === 0,
      'Font notices changed accepted state',
    );
    fontNotices = [];
    await until(
      () => !host.querySelector('[aria-label="Subtitle font notices"]'),
    );
    passed(
      'Session font notices are escaped, read-only and clear with media status',
    );
    assert(field('Cue style').value === 'Title', 'Accepted style missing');
    button('Inspect styles…').click();
    await until(() => !!host.querySelector('dialog[open]'));
    assert(
      host
        .querySelector('[aria-label="Style fields"]')!
        .textContent!.includes('Fontsize36'),
      'Selected style fields missing',
    );
    input('Inspect style', 'Default');
    await tick();
    assert(
      host
        .querySelector('[aria-label="Style fields"]')!
        .textContent!.includes('Fontsize24'),
      'Inspector selection did not change',
    );
    assert(
      edits.length === 0 && field('Cue style').value === 'Title',
      'Inspector assigned a style',
    );
    button('Close styles').click();
    await until(
      () =>
        !host.querySelector('dialog[open]') &&
        !field('Cue style').hasAttribute('disabled'),
    );
    passed(
      'Existing styles and original fields are inspectable without changing the cue',
    );

    const convert = button('Convert subtitles…');
    convert.focus();
    convert.click();
    await until(() => !!host.querySelector('#conversion-dialog-title'));
    assert(
      occluded && button('Apply edit').disabled,
      'Conversion dialog did not occlude native video and protect workspace',
    );
    assert(
      field('Conversion format').value === 'srt' && edits.length === 0,
      'Conversion changed accepted state or selected the source format',
    );
    button('Close conversion').click();
    await until(() => !host.querySelector('dialog[open]') && !convert.disabled);
    assert(
      !occluded && document.activeElement === convert,
      'Conversion close did not restore video and focus',
    );
    await until(() => callbacks.has(eventHandler));
    callbacks.get(eventHandler)!({ payload: 'convertSubtitles' });
    await until(() => !!host.querySelector('#conversion-dialog-title'));
    assert(occluded, 'Native menu conversion skipped modal occlusion');
    button('Close conversion').click();
    await until(() => !host.querySelector('dialog[open]') && !convert.disabled);
    passed(
      'Conversion toolbar and menu share protected modal lifetime and restore native video and focus',
    );

    host.style.width = '820px';
    host.querySelector('main')!.style.height = '610px';
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)),
    );
    await until(() => {
      const panel = host.querySelector('.editor')!.getBoundingClientRect();
      const bar = host.querySelector('.editorbar')!.getBoundingClientRect();
      const text = field('Subtitle text').getBoundingClientRect();
      const footer = host
        .querySelector('.editorfooter')!
        .getBoundingClientRect();
      return (
        bar.bottom <= text.top &&
        text.height >= 34 &&
        text.bottom <= footer.top &&
        footer.bottom <= panel.bottom
      );
    });
    host.style.width = '';
    host.querySelector('main')!.style.height = '';
    await tick();
    passed(
      'Wrapped authoring controls, the textarea and history buttons stay inside the editor panel',
    );

    input('Cue style', 'Default');
    input('Subtitle text', 'Edited\n世界');
    input('Cue start', '00:00:01,010');
    await tick();
    assert(
      doc.cues[0]!.style === 'Title' && !button('Apply edit').disabled,
      'Style change bypassed the draft',
    );
    button('Apply edit').click();
    await until(
      () =>
        edits.length === 1 &&
        button('Apply edit').disabled &&
        !button('Undo edit').disabled,
    );
    assert(
      edits[0]!.kind === 'editStyled' &&
        doc.cues[0]!.style === 'Default' &&
        field('Subtitle text').value === 'Edited\\N世界',
      'Combined accepted reply not reconciled',
    );
    passed(
      'Style, text and timing commit in one request and accepted ASS escapes replace the draft',
    );
    button('Undo edit').click();
    await until(() => field('Cue style').value === 'Title');
    assert(
      field('Subtitle text').value === 'Red title' &&
        field('Cue start').value === '00:00:01,000',
      'Undo did not restore full cue',
    );
    await until(() => !button('Redo edit').disabled);
    button('Redo edit').click();
    await until(() => field('Cue style').value === 'Default');
    passed('One Undo/Redo restores style together with text and timing');

    input('Cue style', 'Title');
    input('Cue start', '00:00:01,001');
    await tick();
    button('Apply edit').click();
    await until(() => host.textContent!.includes('10 ms increments'));
    assert(
      field('Cue style').value === 'Title' &&
        field('Cue start').value === '00:00:01,001' &&
        edits.length === 1,
      'Failed validation lost the draft',
    );
    host.querySelectorAll<HTMLElement>('.cue-row')[1]!.click();
    await until(() => !field('Cue style').hasAttribute('disabled'));
    assert(
      field('Subtitle text').value === 'Edited\\N世界' &&
        field('Cue style').value === 'Title',
      'Failed draft was discarded during navigation',
    );
    button('Revert draft').click();
    await tick();
    assert(
      field('Cue style').value === 'Default' && button('Apply edit').disabled,
      'Revert did not restore accepted style',
    );
    passed(
      'Rejected precision preserves the style/text draft and blocks navigation; Revert restores acceptance',
    );

    input('Cue style', 'Title');
    await tick();
    button('Inspect styles…').click();
    await until(() => !!host.querySelector('dialog[open]'));
    button('Close styles').click();
    await until(
      () =>
        !host.querySelector('dialog[open]') &&
        !field('Cue style').hasAttribute('disabled'),
    );
    assert(
      field('Cue style').value === 'Title' && !button('Apply edit').disabled,
      'Inspector lost an unapplied style',
    );
    button('Revert draft').click();
    await tick();
    passed(
      'Opening and closing the inspector preserves an unapplied style draft',
    );

    input('Cue style', 'Title');
    await tick();
    const editor = field('Subtitle text') as HTMLTextAreaElement;
    editor.focus();
    editor.setSelectionRange(0, 6);
    button('Bold').click();
    await until(() => editor.value.startsWith('{\\b0}Edited{\\b1}'));
    assert(
      editor.selectionStart === 5 &&
        editor.selectionEnd === 11 &&
        doc.cues[0]!.style === 'Default' &&
        edits.length === 1,
      'Formatting committed a draft or lost the source selection',
    );
    await until(() => !button('Revert draft').disabled);
    button('Revert draft').click();
    await tick();
    passed(
      'Formatting captures the draft style and version while preserving explicit Apply',
    );

    let release!: (value: DocumentSnapshot) => void;
    const owner = new DocumentSession({
      chooseFile: async () => null,
      documentAction: async (action) =>
        action.kind === 'status'
          ? original
          : new Promise((resolve) => {
              release = resolve;
            }),
      resyncAccept: async () => original,
      gapAccept: async () => original,
    });
    await owner.command({ kind: 'status' });
    owner.style = 'Default';
    const pending = owner.apply();
    owner.dispose();
    release({ ...original, revision: 99 });
    assert(
      !(await pending) && owner.snapshot.revision === 0,
      'Disposed owner adopted a late styled edit',
    );
    passed('A late styled-edit reply cannot replace a disposed document owner');
    heading.textContent = `PASS: all ${output.querySelectorAll('li').length} ASS/SSA editor checks`;
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
    console.error(error);
  } finally {
    await unmount(app);
  }
}
void run();
