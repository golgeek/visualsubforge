# Linux portal lifecycle regression

From the repository root on Linux, with the pinned Rust toolchain, normal Tauri
build prerequisites, `dbus-daemon`, and system Python's PyGObject/Gio installed:

```sh
/usr/bin/python3 -I tests/native/linux-dialogs/check.py
# Optional: exercise the same test in the native-media build configuration.
/usr/bin/python3 -I tests/native/linux-dialogs/check.py --features media-prototype
```

The harness compiles the actual application test binary with `--locked`, then
runs its opt-in `private_portal_lifecycle` test in a fresh process per private
session bus. Fresh processes avoid reusing ashpd's cached session connection
between buses. No desktop service, display setting, or user file is changed.
The successful path is a synthetic URI, not a file opened or written by the test.

Cases cover a Unicode selection, explicit cancellation, service disconnect,
owner replacement, bus loss, and a successful retry in the same application
process after service restart. Failures occur after the method returns its
request handle but before a Response signal. The mock checks the Open Subtitles
title, single-selection flag, modality flag and SRT filter. The harness bounds
each test to 20 seconds and reaps its own service and bus even after failures.
This timeout belongs to the test; production pickers have no user-interaction
deadline. Bus-loss injection terminates only the bus started by this harness.

These are protocol/lifecycle regressions, not native parenting, modality, Wayland
handle export, Save As, or visible UI evidence. The ignored Rust test is skipped
by ordinary test runs; a zero-test run is not a pass. Use actual desktop checks
for those remaining boundaries. Task003c and decision0011 record their evidence.
