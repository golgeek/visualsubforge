#!/usr/bin/env python3
"""Write the exact independently authored WebVTT library benchmark inputs."""
import hashlib
import json
from pathlib import Path


HASHES = {
    "plain": "337cffd83d4ad10fc053bd565f96b4cbc77fd7c817db39fc40e6d254a5d09198",
    "styled": "783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f",
    "timed": "46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6",
}


def timestamp(ms):
    return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02}.{ms % 1000:03}"


def fixture(kind):
    parts = ["WEBVTT independent 10k fixture\n\n"]
    if kind != "plain":
        parts.append("STYLE\n::cue(.accent) { color: lime; }\n\nREGION\nid:bottom\nwidth:80%\nlines:3\n\n")
    for i in range(10000):
        start = i * 3000 + 1
        if kind != "plain":
            parts.append(f"cue,{i}\n")
        parts.append(f"{timestamp(start)} --> {timestamp(start + 2500)}")
        if kind != "plain":
            parts.append(" line:10%,start position:20%,line-left region:bottom future:keep")
        parts.append("\n")
        if kind == "plain":
            parts.append(f"Bonjour 世界 😀 {i}\nSecond line")
        elif kind == "styled":
            parts.append(f"<v Alice><lang fr><c.accent>Été &amp; 世界 {i}</c></lang></v>\n<ruby>字<rt>じ</rt></ruby>")
        else:
            parts.append(f"<v Alice>Été &amp; 世界 {i} <{timestamp(start + 500)}>first <{timestamp(start + 2000)}>second</v>")
        parts.append("\n\n")
        if i % 100 == 0:
            parts.append("NOTE independent preservation sentinel\n\n")
    raw = "".join(parts)
    if kind == "styled":
        return "\ufeff" + raw.replace("\n", "\r\n")
    if kind == "timed":
        mixed = []
        for i, line in enumerate(raw.splitlines(keepends=True)):
            body = line.removesuffix("\n")
            newline = ("\r", "\n", "\r\n")[i % 3]
            if not body and mixed and mixed[-1].endswith("\r") and newline == "\n":
                mixed.append("\r")
            mixed.extend((body, newline))
        return "".join(mixed)
    return raw


def main():
    root = Path(__file__).resolve().parents[1]
    destination = root / "tests/fixtures/generated/webvtt"
    destination.mkdir(parents=True, exist_ok=True)
    receipt = []
    for kind, expected in HASHES.items():
        data = fixture(kind).encode()
        digest = hashlib.sha256(data).hexdigest()
        if digest != expected:
            raise ValueError(f"{kind}: fixture drift from the measured library workload")
        path = destination / f"10000-{kind}.vtt"
        path.write_bytes(data)
        receipt.append({"kind": kind, "file": path.name, "cues": 10000, "bytes": len(data), "sha256": digest})
    (destination / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
