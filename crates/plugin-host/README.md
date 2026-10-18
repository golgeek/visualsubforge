# Plugin worker foundation

This crate owns bounded JSON transport, helper process lifetime and the native
JavaScript engine boundary. It is not linked into the editor or CLI yet and does
not own subtitle state or history. See [the contract and acceptance record](../../docs/plugin-host.md).

Use `run` from a background application worker with a trusted helper executable;
call `serve` only inside the helper. Engine objects remain on that process's
execution thread. A new runtime/process is used for each request. The embedding
application must separately bound admission and validate all resulting proposals.

The `vsf-plugin-worker` binary is currently the feasibility/test entry point.
Native packaging and the public VSF/VSS APIs remain proposed. Direct execution
does not provide the parent's watchdog, so use the parent API for plugin runs.

No local unsafe code is allowed. `rquickjs`0.14.0 provides the safe binding to its
bundled QuickJS-NG0.16.2 native implementation, both MIT-licensed. Only the `std`
feature is selected; do not enable custom allocation, module loaders or host
capabilities without revisiting bounds and acceptance. Publication must use the
repository's source/notice inventory workflow; dependency names are not a complete
distribution notice or native security audit.
