"""Parse the MSVC linker diagnostic without copying proprietary SDK inputs."""
from pathlib import PureWindowsPath


def input_paths(text):
    paths = []
    for line in text.splitlines():
        line = line.strip().strip('"')
        if not line:
            continue
        # /LINKREPROFULLPATHRSP retains these options before their full paths.
        # Reject unknown options instead of silently losing a linker input.
        for option in ['/defaultlib:', '/natvis:', '/def:']:
            if line.lower().startswith(option):
                line = line[len(option):].strip('"')
                break
        path = PureWindowsPath(line)
        if not path.is_absolute():
            raise ValueError(f'Invalid MSVC linker input: {line}')
        paths.append(path)
    if not paths:
        raise ValueError('Empty MSVC linker input record')
    return sorted(set(paths))
