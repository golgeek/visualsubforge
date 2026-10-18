// Independently authored compatibility example; no historical plugin code.
// GPL-3.0-or-later (repository LICENSE).
globalThis.VSSPlugin = {
  Name: 'Trailing whitespace (VSS example)',
  ParamRemoveTabs: { Value: true },
  HasError(cue) {
    const trailing = this.ParamRemoveTabs.Value ? /[ \t]+$/ : / +$/;
    return trailing.test(cue.Text) ? 'Trailing whitespace' : '';
  },
  FixError(cue) {
    const trailing = this.ParamRemoveTabs.Value ? /[ \t]+$/ : / +$/;
    cue.Text = cue.Text.replace(trailing, '');
  },
};
