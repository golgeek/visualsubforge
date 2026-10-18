#!/usr/bin/env python3
"""Measure the fresh plugin helper and its parent together (macOS/Linux ps)."""
import hashlib
import argparse
import json
import os
import pathlib
import signal
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api", action="store_true", help="Measure complete application API/history workloads")
    options = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    binary = root / "target/release/examples" / ("api-benchmark" if options.api else "runtime-benchmark")
    worker = root / "target/release/vsf-plugin-worker"
    baseline = None
    peak = samples = helper_samples = 0
    phases = {}
    timeout = 300 if options.api else 90
    deadline = time.monotonic() + timeout
    with tempfile.TemporaryFile(mode="w+") as output, tempfile.TemporaryFile(mode="w+") as errors:
        child = subprocess.Popen([str(binary)], stdout=output, stderr=errors)
        try:
            while child.poll() is None:
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"Plugin benchmark exceeded {timeout}s")
                output.seek(0)
                progress = output.read()
                phase = next((line[6:] for line in reversed(progress.splitlines()) if line.startswith("phase ")), "fixture")
                snapshot = subprocess.run(["ps", "-ax", "-o", "pid=,ppid=,rss="], capture_output=True, text=True, check=True, timeout=2)
                rows = [tuple(map(int, line.split())) for line in snapshot.stdout.splitlines()]
                owners = [(pid, rss) for pid, parent, rss in rows if pid == child.pid or parent == child.pid]
                if progress.startswith("fixture_ready ") and any(pid == child.pid for pid, _ in owners):
                    rss = sum(value for _, value in owners) * 1024
                    if baseline is None:
                        if phase != "fixture":
                            raise RuntimeError("Missed parent baseline before helper start")
                        baseline = rss
                    peak = max(peak, rss)
                    samples += 1
                    helper_samples += int(len(owners) > 1)
                    phases[phase] = max(phases.get(phase, 0), rss)
                time.sleep(0.01)
            output.seek(0)
            text = output.read()
            errors.seek(0)
            lines = text.splitlines()
            if child.returncode != 0 and (not lines or not lines[-1].startswith("{")):
                raise RuntimeError(f"Benchmark failed: {text}\n{errors.read()}")
            if baseline is None or helper_samples < 10:
                raise RuntimeError("Insufficient live-helper RSS observations")
            result = json.loads(text.splitlines()[-1])
            result.update({
                "exit_code": child.returncode,
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "worker_sha256": hashlib.sha256(worker.read_bytes()).hexdigest(),
                "rss_baseline_bytes": baseline,
                "rss_peak_bytes": peak,
                "rss_increase_bytes": peak - baseline,
                "rss_phase_peaks_bytes": phases,
                "rss_samples": samples,
                "rss_helper_samples": helper_samples,
                "rss_budget_bytes": 192 * 1024 * 1024,
                "memory_passed": peak - baseline <= 192 * 1024 * 1024,
                "memory_method": "10ms sampled ps RSS, benchmark parent plus direct helper; fixture allocation precedes baseline; sampling may miss peaks; not an OS RSS limit",
            })
            print(json.dumps(result, indent=2))
            if child.returncode != 0 or not result["passed"] or not result["memory_passed"]:
                raise RuntimeError("Predeclared plugin runtime budget failed")
        finally:
            if child.poll() is None:
                # Each run has its own <=1s helper deadline. Give that owner a
                # chance to reap before stopping an unexpectedly hung benchmark.
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    snapshot = subprocess.run(["ps", "-ax", "-o", "pid=,ppid="], capture_output=True, text=True, check=True, timeout=2)
                    for line in snapshot.stdout.splitlines():
                        pid, parent = map(int, line.split())
                        if parent == child.pid:
                            try:
                                os.kill(pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                    try:
                        child.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        child.kill()
            child.wait()


if __name__ == "__main__":
    main()
