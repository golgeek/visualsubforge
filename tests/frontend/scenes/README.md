# Scene controls regression

With pinned Node, run `npm run dev` and open
`http://localhost:1420/tests/frontend/scenes/`.
Expect **PASS: all 18 scene checks**. The production component and typed client
use modeled IPC: no automatic keyframe scan, legacy keyframe exclusion, completion, visibility, previous/next navigation, cancellation,
error/retry and stale start after media replacement. No native detection claim;
run the FFmpeg scene integrations in [CONTRIBUTING](../../../CONTRIBUTING.md#browser-and-analysis-regressions) for actual detection checks.
Current native evidence and limits are in [status](../../../docs/status.md).

Also checks detail selection and restored-generation stability during repeated media status polling.
