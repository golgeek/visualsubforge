"""Check actual dynamic-loader bindings in the installed Ubuntu GUI process."""
from collections import defaultdict
import json
from pathlib import Path
import re

BINDING = re.compile(r"binding file (.+?) \[\d+\] to (.+?) \[\d+\]:.*?symbol [`']([^']+)'")
XML = re.compile(r'libxml2\.so\.(2|16)(?:\.|$)')


def audit(lines, mapped, private, output):
    bindings = defaultdict(set)
    failures = []
    saw_ffmpeg = saw_webkit = False
    for line in lines:
        if 'libxml2.so.' not in line or not (match := BINDING.search(line)):
            continue
        caller, provider = (Path(value).resolve() for value in match.groups()[:2])
        abi = XML.match(provider.name)
        if not abi:
            continue
        bindings[(str(caller), str(provider))].add(match[3])
        private_caller = caller.is_relative_to(private)
        expected = '2' if private_caller else '16'
        if abi[1] != expected or provider.is_relative_to(private) != private_caller:
            failures.append(f'{caller} -> {provider}')
        if private_caller and caller.name.startswith('libavformat.so.') and match[3].startswith('xml'):
            saw_ffmpeg = True
        if not private_caller and caller.name.startswith('libwebkit2gtk-') and match[3].startswith('xml'):
            saw_webkit = True
    paths = sorted(p for p in mapped if XML.match(Path(p).name))
    if not (saw_ffmpeg and saw_webkit):
        failures.append('Missing XML binding evidence for FFmpeg or WebKitGTK')
    if {XML.match(Path(p).name)[1] for p in paths} != {'2', '16'}:
        failures.append('Both XML ABIs must be mapped in the GUI process')
    report = {'mapped_xml_libraries': paths, 'bindings': [
        {'consumer': c, 'provider': p, 'symbols': sorted(symbols)}
        for (c, p), symbols in sorted(bindings.items())],
        'failures': sorted(set(failures))}
    output.write_text(json.dumps(report, indent=2) + '\n')
    if failures:
        raise ValueError(f'Ubuntu XML binding check failed; see {output}')
