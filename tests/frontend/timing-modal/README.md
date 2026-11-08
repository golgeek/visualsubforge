# Timing modal lifecycle

Mounts the production App with controlled IPC in both Chromium and WebKit.
All four timing variants exercise delayed above-WebView hide/restore, competing
workspace actions, return focus, retained diagnostics, disposal, and draft/accept
acknowledgment guards. Additional cases cover transition errors, late admission,
a detached launcher and below-WebView capability. Existing timing feature fixtures
retain calculation, paging, stale-context, failed-acceptance and history coverage.

`workspace-lifecycle/modals.ts` separately tests close during admission and disposal
at rendering boundaries. `gap-review/main.ts` includes the launcher-focus regression.
These tests model native occlusion acknowledgments; they do not prove physical
native surface composition or input behavior.

Run `npm run test:browser -- --grep 'workflow: timing-modal$'` with the pinned
prerequisites in the desktop test guide.
