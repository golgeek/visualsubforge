#!/usr/bin/env python3
"""Verify the exact licensed dictionary payloads used by the application."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / 'crates/vsf-application/assets/dictionaries'
manifest = json.loads((root / 'sources.json').read_text())
assert manifest['schema'] == 1
assert manifest['commit'] == '67032ee957631908098a72edf48dc81cac27e369'
for entry in manifest['files']:
    path = (root / entry['path']).resolve()
    path.relative_to(root)
    data = path.read_bytes()
    assert len(data) == entry['bytes'], f"Size differs: {entry['path']}"
    assert hashlib.sha256(data).hexdigest() == entry['sha256'], f"SHA256 differs: {entry['path']}"
    print(f"Verified {entry['path']}: {entry['sha256']}")
assert 'CLASSIQUE' in (root / 'fr.aff').read_text()
print('Dictionary data and complete upstream notices verified.')
