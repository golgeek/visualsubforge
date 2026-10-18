#!/usr/bin/env python3
"""Fresh CLI check/preview/apply, complete 10k review/output checks and sampled RSS."""
import hashlib
import json
import math
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/release/vsf"
BUDGET_MS = 500
BUDGET_RSS = 192 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def invoke(args, expected):
    samples = helper_samples = peak = 0
    started = time.monotonic()
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        child = subprocess.Popen([str(BINARY), *map(str, args)], stdin=subprocess.DEVNULL,
                                 stdout=output, stderr=errors)
        try:
            while child.poll() is None:
                if time.monotonic() - started > 15:
                    raise RuntimeError("CLI exceeded benchmark watchdog")
                snapshot = subprocess.run(["ps", "-ax", "-o", "pid=,ppid=,rss="],
                                          capture_output=True, text=True, check=True, timeout=2)
                rows = [tuple(map(int, line.split())) for line in snapshot.stdout.splitlines()]
                owners = [(pid, rss) for pid, parent, rss in rows
                          if pid == child.pid or parent == child.pid]
                if owners:
                    samples += 1
                    helper_samples += int(len(owners) > 1)
                    peak = max(peak, sum(rss for _, rss in owners) * 1024)
                time.sleep(0.005)
            elapsed = (time.monotonic() - started) * 1000
            output.seek(0)
            errors.seek(0)
            stdout, stderr = output.read(), errors.read()
            if child.returncode != expected or stderr:
                raise RuntimeError(f"CLI returned {child.returncode}: {stderr.decode(errors='replace')}")
            return json.loads(stdout), elapsed, peak, samples, helper_samples
        finally:
            if child.poll() is None:
                child.send_signal(signal.SIGINT)
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    # Kill only children observed under this benchmark's CLI.
                    snapshot = subprocess.run(["ps", "-ax", "-o", "pid=,ppid="],
                                              capture_output=True, text=True, check=True, timeout=2)
                    for line in snapshot.stdout.splitlines():
                        pid, parent = map(int, line.split())
                        if parent == child.pid:
                            try:
                                os.kill(pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                    child.kill()
            child.wait()


def main():
    dense = "".join(f"{i}\n00:00:01,000 --> 00:00:05,000\n{{\\an8\\k20}}<i>Été 世界 😀 {i}</i>\nSecond line\n\n"
                    for i in range(1, 10001)).encode()
    fixtures = [("dense-srt", "srt", dense)]
    for name, extension, path in [
        ("ass", "ass", "ass-ssa/10000.ass"),
        ("ssa", "ssa", "ass-ssa/10000.ssa"),
        ("styled-vtt", "vtt", "webvtt/10000-styled.vtt"),
        ("timed-vtt", "vtt", "webvtt/10000-timed.vtt"),
    ]:
        fixtures.append((name, extension, (ROOT / "tests/fixtures/generated" / path).read_bytes()))
    reports = []
    samples = helper_samples = peak = 0
    with tempfile.TemporaryDirectory(prefix="vsf-plugin-cli-bench-") as directory:
        root = pathlib.Path(directory)
        package = root / "package"
        package.mkdir()
        manifest = package / "manifest.json"
        for name, extension, data in fixtures:
            source = root / f"source.{extension}"
            output = root / f"output.{extension}"
            source.write_bytes(data)
            for api in ["vsf-1", "vss-1"]:
                manifest.write_text(json.dumps({"version": 1, "id": "benchmark.append", "name": "Append marker",
                                                "api": api, "entry": "main.js", "requires": ["rules", "fixes"]}))
                # These independently authored rules return raw text as findings,
                # allowing a second real parse/check to verify every output cue.
                script = ("VSFPlugin={check(c){return c.text},fix(c,h){h.update(c.id,{text:c.text+'!'})}}"
                          if api == "vsf-1" else
                          "VSSPlugin={Name:'Append marker',HasError(c){return c.Text},FixError(c){c.Text+='!'}}")
                (package / "main.js").write_text(script)
                common = ["--subtitles", source, "--package", manifest, "--json"]
                measurements = {action: [] for action in ["check", "preview", "apply"]}
                phase_peak = 0
                for iteration in range(21):
                    before = preview = None
                    for action in measurements:
                        args = ["plugin", action, *common]
                        if action == "apply":
                            args += ["--accept", preview["reviewId"], "--output", output]
                        result, elapsed, rss, count, live = invoke(args, 1 if action == "check" else 0)
                        if iteration > 0:
                            measurements[action].append(elapsed)
                        samples += count
                        helper_samples += live
                        peak = max(peak, rss)
                        phase_peak = max(phase_peak, rss)
                        if action == "check":
                            assert len(result["findings"]) == 10000 and not result["changes"]
                            before = [f["message"].replace("\r\n", "\n") for f in result["findings"]]
                        elif action == "preview":
                            assert len(result["findings"]) == len(result["changes"]) == 10000
                            assert len({r["cueId"] for r in result["changes"]}) == 10000
                            for index, row in enumerate(result["changes"]):
                                assert row["cue"] == index + 1 and not row["outsideScope"]
                                assert row["before"]["text"] == before[index]
                                assert row["after"]["text"] == before[index] + "!"
                                for key in ["startMs", "endMs"]:
                                    assert row["before"][key] == row["after"][key]
                            preview = result
                        else:
                            assert result["status"] == "published"
                            assert result["reviewId"] == preview["reviewId"]
                            assert digest(output.read_bytes()) == result["outputSha256"] == preview["outputSha256"]
                        assert source.read_bytes() == data
                    reopened, _, rss, count, live = invoke(
                        ["plugin", "check", "--subtitles", output, "--package", manifest, "--json"], 1)
                    assert [f["message"].replace("\r\n", "\n") for f in reopened["findings"]] == [s + "!" for s in before]
                    samples += count
                    helper_samples += live
                    peak = max(peak, rss)
                    phase_peak = max(phase_peak, rss)
                    output.unlink()
                for action, times in measurements.items():
                    times.sort()
                    p95 = times[math.ceil(len(times) * 0.95) - 1]
                    reports.append({"name": f"{name}-{api}-{action}", "input_sha256": digest(data),
                                    "input_bytes": len(data), "cues": 10000, "warmups": 1, "iterations": 20,
                                    "p95_ms": p95, "max_ms": max(times), "sampled_phase_peak_bytes": phase_peak,
                                    "passed": p95 <= BUDGET_MS})
                print(f"Completed {name} {api}", file=sys.stderr, flush=True)
    result = {"schema": 1, "scope": "Fresh CLI startup, package/file IO, helper, full report or atomic publication, input revalidation. Complete raw rows, source/output hashes and real output reparse checked outside timer. No desktop/native/recovery acceptance.",
              "workloads": reports, "binary_sha256": digest(BINARY.read_bytes()),
              "p95_budget_ms": BUDGET_MS, "rss_budget_bytes": BUDGET_RSS,
              "rss_baseline_bytes": 0, "rss_peak_bytes": peak,
              "rss_samples": samples, "rss_helper_samples": helper_samples,
              "memory_method": "Sampled ps RSS of fresh CLI plus its direct helper; no baseline subtraction. Polling adds wall time; sampling may miss peaks. Not an OS memory cap.",
              "passed": all(r["passed"] for r in reports) and peak <= BUDGET_RSS and helper_samples >= 10}
    print(json.dumps(result, indent=2))
    if not result["passed"]:
        raise SystemExit("Predeclared CLI latency/memory gate failed")


if __name__ == "__main__":
    main()
