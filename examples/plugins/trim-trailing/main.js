// Independently authored VSF example, GPL-3.0-or-later (repository LICENSE).
globalThis.VSFPlugin = {
  check(cue, host) {
    const trailing = host.settings.removeTabs ? /[ \t]+$/ : / +$/;
    return trailing.test(cue.text) ? 'Trailing whitespace' : '';
  },
  fix(cue, host) {
    const trailing = host.settings.removeTabs ? /[ \t]+$/ : / +$/;
    host.update(cue.id, { text: cue.text.replace(trailing, '') });
  },
};
