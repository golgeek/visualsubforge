/* global LoadScript, VSSCore, SceneChange, contractHex */
// Independently authored cross-host probe. GPL-3.0-or-later (repository LICENSE).
// Keep ES3 syntax so this same file can run in the historical SpiderMonkey host.
LoadScript('helpers/encoding.js');
var contractCalls = 0;
var contractExpression = /e/g;
var VSSPlugin = {
  Name: 'VSF independently authored contract probe',
  Description: 'Reports synthetic subtitle API observations for comparison.',
  Color: 0x336699,
  Message: 'Contract observation',
  ParamMarker: { Value: 7, Unit: '', Description: 'Scalar parameter control' },
  HasError: function (cue, previous, next) {
    contractCalls += 1;
    var selected = [];
    var item = VSSCore.GetFirstSelected();
    while (item && selected.length < VSSCore.GetSubCount()) {
      selected.push(item.Index);
      item = VSSCore.GetNextSelected(item);
    }
    return [
      'index=' + cue.Index,
      'start=' + cue.Start,
      'stop=' + cue.Stop,
      'raw=' + contractHex(cue.Text),
      'stripped=' + contractHex(cue.StrippedText),
      'previous=' + (previous ? previous.Index : 0),
      'next=' + (next ? next.Index : 0),
      'count=' + VSSCore.GetSubCount(),
      'at=' + VSSCore.GetSubAt(cue.Index - 1).Index,
      'first=' + VSSCore.GetFirst().Index,
      'previousCore=' +
        (VSSCore.GetPrevious(cue) ? VSSCore.GetPrevious(cue).Index : 0),
      'nextCore=' + (VSSCore.GetNext(cue) ? VSSCore.GetNext(cue).Index : 0),
      'bounds=' +
        (VSSCore.GetSubAt(-1) === null && VSSCore.GetSubAt(3) === null),
      'selectedCount=' + VSSCore.GetSelectedCount(),
      'selected=' + selected.join(','),
      'cursor=' + VSSCore.GetAudioCursorPosition(),
      'preferences=' +
        [
          VSSCore.MinimumBlank,
          VSSCore.MinimumDuration,
          VSSCore.MaximumDuration,
          VSSCore.CpsTarget,
          VSSCore.RsTargetIndex,
        ].join(','),
      'scenes=' +
        [
          SceneChange.GetCount(),
          SceneChange.GetNext(0),
          SceneChange.GetPrevious(10000),
          SceneChange.Contains(0, 10000),
          SceneChange.StartOffset,
          SceneChange.StopOffset,
          SceneChange.FilterOffset,
        ].join(','),
      'calls=' + contractCalls,
      'regexp=' + contractExpression.test('e'),
      'parameter=' + this.ParamMarker.Value,
      'receiver=' + (this === VSSPlugin),
    ].join(';');
  },
  FixError: function (cue, previous, next) {
    cue.Start += 10;
    cue.Stop += 10;
    cue.Text += '!';
    if (next) next.Text += ' neighbour';
  },
};
