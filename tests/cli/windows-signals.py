#!/usr/bin/env python3
"""Exercise the real Windows CLI's Ctrl-C and timeout cleanup.

Run after cargo build --locked -p vsf-cli:
    python tests/cli/windows-signals.py --vsf target/debug/vsf.exe

Each worker receives a private console before it launches VSF. Ctrl-C is a
console-wide event, not a signal addressable to one PID; never broadcast into
the caller's console. The controlled analysis child ignores Ctrl-C, so its death
proves CLI cleanup rather than an independent reaction to the console event.
See https://learn.microsoft.com/windows/console/generateconsolectrlevent and
https://learn.microsoft.com/windows/console/setconsolectrlhandler .
No FFmpeg, media fixture, GUI or native playback dependency is needed.
"""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

SRT = "7\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n"
ROOT = Path(__file__).resolve().parents[2]


def worker(vsf, directory, mode):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)

    def api(name, args, result):
        function = getattr(kernel, name)
        function.argtypes, function.restype = args, result
        return function

    console_processes = api("GetConsoleProcessList", [ctypes.POINTER(wintypes.DWORD), wintypes.DWORD], wintypes.DWORD)
    control_handler = api("SetConsoleCtrlHandler", [ctypes.c_void_p, wintypes.BOOL], wintypes.BOOL)
    send_interrupt = api("GenerateConsoleCtrlEvent", [wintypes.DWORD, wintypes.DWORD], wintypes.BOOL)
    open_process = api("OpenProcess", [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD], wintypes.HANDLE)
    wait = api("WaitForSingleObject", [wintypes.HANDLE, wintypes.DWORD], wintypes.DWORD)
    terminate = api("TerminateProcess", [wintypes.HANDLE, wintypes.UINT], wintypes.BOOL)
    close = api("CloseHandle", [wintypes.HANDLE], wintypes.BOOL)

    members = (wintypes.DWORD * 16)()
    count = console_processes(members, len(members))
    assert count == 1 and members[0] == os.getpid(), "Worker must own a private console"
    # Clear any inherited ignore flag; adding a handler alone does not clear it.
    assert control_handler(None, False), ctypes.get_last_error()
    handler_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.DWORD)
    handler = handler_type(lambda event: event == 0)
    assert control_handler(ctypes.cast(handler, ctypes.c_void_p), True), ctypes.get_last_error()

    case = directory / mode
    case.mkdir()
    inputs = {"source.srt": SRT.encode(), "source.mkv": b"source", "target.mkv": b"target"}
    for name, content in inputs.items():
        (case / name).write_bytes(content)
    pid_path = case / "tool.pid"
    env = os.environ.copy()
    env["VSF_FFMPEG"] = str(directory / "slow-tool.exe")
    env["VSF_TEST_CHILD_PID"] = str(pid_path)
    command = [str(vsf), "resync", "--source-video", "source.mkv",
               "--source-subtitles", "source.srt", "--target-video", "target.mkv",
               "--output", "result.srt", "--report", "report.json",
               "--timeout-seconds", "30" if mode == "ctrl-c" else "2"]
    started = time.monotonic()
    child = subprocess.Popen(command, cwd=case, env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    tool_handle = None
    try:
        deadline = time.monotonic() + 10
        while True:
            if pid_path.exists():
                value = pid_path.read_text()
                if value.isdecimal():
                    tool_pid = int(value)
                    break
            assert child.poll() is None, "CLI exited before the controlled tool started"
            assert time.monotonic() < deadline, "Controlled tool did not start"
            time.sleep(0.01)
        # Keep the process handle through the assertion and cleanup; never use
        # a later PID lookup that could refer to a reused identifier.
        tool_handle = open_process(0x00100001, False, tool_pid)  # SYNCHRONIZE | TERMINATE
        assert tool_handle, ctypes.get_last_error()
        assert wait(tool_handle, 0) == 0x102, "Tool must be alive before cancellation"
        if mode == "ctrl-c":
            assert send_interrupt(0, 0), ctypes.get_last_error()
        stdout, stderr = child.communicate(timeout=10)
        assert child.returncode == (130 if mode == "ctrl-c" else 7), stderr.decode(errors="replace")
        assert not stdout, stdout
        assert wait(tool_handle, 0) == 0, "Analysis child survived CLI exit"
        assert not (case / "result.srt").exists()
        assert not (case / "report.json").exists()
        for name, content in inputs.items():
            assert (case / name).read_bytes() == content, name
        print(json.dumps({"case": mode, "exit": child.returncode,
                          "elapsed_seconds": time.monotonic() - started,
                          "child_terminated": True, "inputs_unchanged": True}), flush=True)
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        if tool_handle:
            if wait(tool_handle, 0) == 0x102:
                terminate(tool_handle, 1)
                wait(tool_handle, 5000)
            close(tool_handle)
        control_handler(ctypes.cast(handler, ctypes.c_void_p), False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vsf", type=Path, required=True)
    parser.add_argument("--worker", choices=["ctrl-c", "timeout"], help=argparse.SUPPRESS)
    parser.add_argument("--directory", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This check requires Windows; it must not silently skip")
    vsf = args.vsf.resolve(strict=True)
    if args.worker:
        worker(vsf, args.directory, args.worker)
        return
    with tempfile.TemporaryDirectory(prefix="vsf-windows-signals-") as temporary:
        directory = Path(temporary)
        subprocess.run(["rustc", "--edition=2024", "-D", "warnings",
                        str(ROOT / "tests/cli/windows-slow-tool.rs"),
                        "-o", str(directory / "slow-tool.exe")], cwd=ROOT, check=True)
        for mode in ["ctrl-c", "timeout"]:
            startup = subprocess.STARTUPINFO()
            startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
            startup.wShowWindow = subprocess.SW_HIDE
            result = subprocess.run([sys.executable, str(Path(__file__).resolve()),
                                     "--vsf", str(vsf), "--worker", mode,
                                     "--directory", str(directory)],
                                    stdin=subprocess.DEVNULL, capture_output=True,
                                    creationflags=subprocess.CREATE_NEW_CONSOLE,
                                    startupinfo=startup)
            if result.returncode:
                raise RuntimeError(result.stderr.decode(errors="replace"))
            print(result.stdout.decode().strip())


if __name__ == "__main__":
    main()
