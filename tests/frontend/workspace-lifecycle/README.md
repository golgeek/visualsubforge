# Workspace owner lifecycle regressions

With pinned Node and installed Playwright browsers, run:

```sh
npm run test:browser -- --grep 'workflow: workspace-lifecycle'
```

Or run `npm run dev` and open `/tests/frontend/workspace-lifecycle/`.
The page compiles the production Svelte session owners and supplies controlled
client replies. It checks draft/accepted-state isolation, rejected and overlapping
document commands, captured gesture versions, selection reconciliation, media
response ordering, loop draining, polling disposal, project restore/quit ordering,
and disposal during each project step. Existing App fixtures separately exercise
real component bindings, dialogs, gestures and workflows.

The summary reports its actual completed assertion count. The automated runner
requires matching PASS rows, no FAIL row or uncaught page error, and completion
before its timeout. These are frontend lifecycle tests, not native cancellation,
filesystem persistence, playback or GPU/audio acceptance.
