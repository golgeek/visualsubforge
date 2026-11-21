"""Check the packaged GUI's glibc bindings and mapped shared objects."""
from collections import Counter, defaultdict
from functools import lru_cache
import json
from pathlib import Path
import re

BINDING = re.compile(r"binding file (.+?) \[\d+\] to (.+?) \[\d+\]:.*?symbol [`']([^']+)'")
SHARED = re.compile(r"^(.+?)\.so(?:\..*)?$")


def audit(lines, mapped, private, output):
    private = private.resolve()
    shipped = {m[1] for p in private.iterdir() if (m := SHARED.match(p.name))}
    families = defaultdict(set)
    pairs = Counter()
    failures = set()

    @lru_cache(maxsize=None)
    def identify(name):
        path = Path(name)
        if not path.is_absolute():
            return path, None
        path = path.resolve()
        match = SHARED.match(path.name)
        if match:
            family = match[1]
            families[family].add(str(path))
            if (family in shipped) != path.is_relative_to(private):
                failures.add(f"Unexpected provider of {family}: {path}")
            return path, family
        return path, None

    for name in mapped:
        identify(name)
    for line in lines:
        match = BINDING.search(line)
        if not match:
            continue
        caller, consumer = identify(match[1])
        provider, library = identify(match[2])
        if not library:
            continue
        pairs[str(caller), str(provider)] += 1
        if consumer and not caller.is_relative_to(private) and library in shipped:
            failures.add(f"System library bound a private symbol: {caller} -> {provider}: {match[3]}")
    for family, paths in families.items():
        if len(paths) > 1:
            failures.add(f"Multiple providers of {family}: {sorted(paths)}")
    if not all(any(family in path for (_, path) in pairs) for family in
               ("libwebkit2gtk-", "libmpv.so", "libavcodec.so")):
        failures.add("Missing loader binding evidence for WebKitGTK, mpv or FFmpeg")
    report = {"families": {name: sorted(paths) for name, paths in sorted(families.items())},
              "bindings": [{"consumer": c, "provider": p, "count": count}
                           for (c, p), count in sorted(pairs.items())],
              "failures": sorted(failures)}
    output.write_text(json.dumps(report, indent=2) + "\n")
    if failures:
        raise ValueError(f"Library uniqueness failed: {sorted(failures)[:5]} (see {output})")
