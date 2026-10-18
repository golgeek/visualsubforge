#!/usr/bin/env python3
"""Deterministic 10,000-cue ASS/SSA inputs for the predeclared format budget."""
import hashlib
from pathlib import Path


def timestamp(milliseconds):
    return f"{milliseconds // 3600000}:{milliseconds // 60000 % 60:02}:{milliseconds // 1000 % 60:02}.{milliseconds % 1000 // 10:02}"


def main():
    root = Path(__file__).resolve().parent.parent
    destination = root / "tests/fixtures/generated/ass-ssa"
    destination.mkdir(parents=True, exist_ok=True)
    for extension in ("ass", "ssa"):
        template = (root / f"tests/fixtures/subtitles/styles.{extension}").read_text()
        header = template.split("[Events]\n")[0]
        lines = [header, "[Events]\n"]
        if extension == "ass":
            lines.append("Format: Style, End, Start, Layer, Name, MarginL, MarginR, MarginV, Effect, Extra, Text\n")
        else:
            lines.append("Format: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
        for index in range(10000):
            start = timestamp(index * 1500)
            end = timestamp(index * 1500 + 1000)
            style = "Title" if index % 4 == 1 else "Default"
            text = f"{{\\i1}}Cue {index + 1:05} — 日本語 😀{{\\i0}}\\NSecond line"
            if extension == "ass":
                fields = f"{style},{end},{start},{index % 3},actor-{index % 7},0000,0000,0000,,opaque-{index % 5},{text}"
            else:
                fields = f"Marked=0,{start},{end},{style},actor-{index % 7},0000,0000,0000,,{text}"
            lines.append(f"Dialogue: {fields}\n")
            if index % 1000 == 999:
                lines.append(f"; Opaque anchor after cue {index + 1}\n")
        lines.append("[Unknown]\nOpaque: preserved,not an editable event\n")
        source = "".join(lines).encode()
        path = destination / f"10000.{extension}"
        path.write_bytes(source)
        print(f"{path.name}: cues=10000 bytes={len(source)} sha256={hashlib.sha256(source).hexdigest()}")


if __name__ == "__main__":
    main()
