#!/usr/bin/env python3
"""Measure the prebuilt text benchmark's CPU and sampled RSS budgets.

macOS/Linux only: ps reports KiB. This is application-library evidence, not UI
input latency or GPU validation. Build the release example before running.
"""
import json
import pathlib
import subprocess
import tempfile
import time


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    binary = root / "target/release/examples/text-benchmark"
    deadline = time.monotonic() + 90
    baseline = None
    peak = 0
    samples = 0
    with tempfile.TemporaryFile(mode="w+") as stdout, tempfile.TemporaryFile(mode="w+") as stderr:
        child = subprocess.Popen([str(binary)], stdout=stdout, stderr=stderr, text=True)
        try:
            while child.poll() is None:
                if time.monotonic() > deadline:
                    raise RuntimeError("Text benchmark exceeded 90-second observation limit")
                stdout.seek(0)
                ready = stdout.read().startswith("fixture_ready ")
                memory = subprocess.run(
                    ["ps", "-o", "rss=", "-p", str(child.pid)],
                    capture_output=True, text=True, timeout=2, check=False,
                )
                if ready and memory.returncode == 0 and memory.stdout.strip():
                    rss = int(memory.stdout.strip()) * 1024
                    if baseline is None:
                        baseline = rss
                    peak = max(peak, rss)
                    samples += 1
                time.sleep(0.01)
            stdout.seek(0)
            output = stdout.read()
            stderr.seek(0)
            errors = stderr.read()
            if child.returncode != 0:
                raise RuntimeError(f"Benchmark exit {child.returncode}: {output}\n{errors}")
            result = json.loads(output.splitlines()[-1])
            if baseline is None or samples < 3:
                raise RuntimeError("Insufficient RSS observations")
            result.update({
                "rss_baseline_bytes": baseline,
                "rss_peak_bytes": peak,
                "rss_increase_bytes": peak - baseline,
                "rss_samples": samples,
                "rss_budget_bytes": 96 * 1024 * 1024,
                "memory_passed": peak - baseline < 96 * 1024 * 1024,
                "fixture": output.splitlines()[0],
                "memory_method": "sampled ps RSS; not exhaustive allocation accounting",
            })
            print(json.dumps(result, indent=2))
            if not result["passed"] or not result["memory_passed"]:
                raise RuntimeError("Predeclared text budget failed")
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()


if __name__ == "__main__":
    main()
