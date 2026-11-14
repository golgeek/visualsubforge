// Generated keyboard input through the actual CueEditor handlers. These helpers
// replace removed Apply/Revert buttons; they never invoke a document command.
export async function installEditorInput(browser) {
  await browser.execute(() => {
    const dispatch = async (key, ctrlKey = false) => {
      const editor = document.querySelector('[aria-label="Subtitle text"]');
      if (!editor || editor.disabled) throw new Error('Cue editor unavailable');
      editor.dispatchEvent(
        new KeyboardEvent('keydown', {
          key,
          ctrlKey,
          bubbles: true,
          cancelable: true,
        }),
      );
      await new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      );
    };
    window.__vsfEditorInput = {
      commit: () => dispatch('Enter', true),
      discard: () => dispatch('Escape'),
    };
  });
}
export function commitCue(browser) {
  return browser.execute(() => window.__vsfEditorInput.commit());
}
export function discardCue(browser) {
  return browser.execute(() => window.__vsfEditorInput.discard());
}
