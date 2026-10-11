"""Run the production Linux dialog adapter against disposable session buses."""

import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile

TEST = "linux_dialog::tests::private_portal_lifecycle"
CASES = ("success", "cancel", "disconnect", "replace", "bus-loss", "restart")


def stop(process):
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def ready_line(process):
    if not select.select([process.stdout], [], [], 10)[0]:
        raise RuntimeError("private service startup timed out")
    return process.stdout.readline().strip()


def check_case(executable, case):
    # Logs are temporary, and included in failure output. No user files or
    # persistent bus configuration are touched. Every owned process is reaped.
    with tempfile.TemporaryFile(mode="w+") as errors:
        bus = subprocess.Popen(
            ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
            stdout=subprocess.PIPE, stderr=errors, text=True,
        )
        portal = None
        try:
            address = ready_line(bus)
            assert address.startswith("unix:"), address
            env = dict(
                os.environ, DBUS_SESSION_BUS_ADDRESS=address,
                VSF_PORTAL_TEST_CASE=case, VSF_PORTAL_TEST_BUS_PID=str(bus.pid),
            )
            portal = subprocess.Popen(
                [sys.executable, "-I", str(Path(__file__).with_name("portal.py"))],
                env=env, stdout=subprocess.PIPE, stderr=errors, text=True,
            )
            assert ready_line(portal) == "READY"
            result = subprocess.run(
                [executable, "--exact", TEST, "--ignored", "--nocapture"],
                env=env, capture_output=True, text=True, timeout=20,
            )
            assert result.returncode == 0, result.stdout + result.stderr
            assert "1 passed" in result.stdout, result.stdout
            stop(portal)
            events = [json.loads(line) for line in portal.stdout]
            expected = ["request", "completed"] * (2 if case == "restart" else 1)
            assert [event["event"] for event in events] == expected, events
            print(json.dumps({"case": case, "passed": True, "events": events}), flush=True)
        except BaseException:
            errors.seek(0)
            print(errors.read(), file=sys.stderr)
            raise
        finally:
            if portal is not None:
                stop(portal)
                portal.stdout.close()
            stop(bus)
            bus.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--features", default="")
    args = parser.parse_args()
    assert sys.platform == "linux", "Linux session-bus integration test"
    command = [
        "cargo", "test", "--locked", "-p", "visualsubforge", "--no-run",
        "--message-format=json",
    ]
    if args.features:
        command.extend(["--features", args.features])
    build = subprocess.run(command, capture_output=True, text=True)
    if build.returncode:
        print(build.stdout + build.stderr, file=sys.stderr)
        build.check_returncode()
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    executables = [
        item["executable"] for item in artifacts
        if item.get("reason") == "compiler-artifact"
        and item["target"]["name"] == "visualsubforge"
        and item["profile"]["test"] and item.get("executable")
    ]
    assert len(executables) == 1, executables
    for case in CASES:
        check_case(executables[0], case)


if __name__ == "__main__":
    main()
