# Synthetic comparison pairs

These small, invented captions contain no personal subtitles or media. The case
manifest specifies exact classifications and options; `expected.json` records the
complete version-1 JSON contract, including token scripts, content snapshots,
metrics and keys. The hostile-text case also records the exact standalone HTML
with a fixed date. Run:

```sh
cargo test --locked -p vsf-application --test comparison
cargo test --locked -p vsf-application comparison::tests -- --nocapture
cargo test --locked -p visualsubforge document::review
```

The second command includes the synthetic 10k/5% comparison budget. Desktop
owner tests separately exercise structural revert and ordinary Undo.
