# English and French language tools

Task 021 is in progress. The dictionary/engine feasibility probe is implemented;
profile/Problems integration, reviewed corrections, personal/ignored words and
native workflow acceptance remain to be completed. This page does not claim an
available end-to-end spelling feature.

## Pinned engine and data

The candidate is [Spellbook 0.4.2](https://docs.rs/spellbook/0.4.2/spellbook/), a
Rust Hunspell-format engine under MPL-2.0. Its upstream still labels it alpha.
The application adds no native spellchecker dependency. Unicode lookup uses
NFC from unicode-normalization 0.1.26 and word boundaries from
unicode-segmentation 1.13.3; source subtitle text will remain unchanged until an
explicit correction is accepted.

Data is pinned to LibreOffice dictionary commit
`67032ee957631908098a72edf48dc81cac27e369`: US English SCOWL 2020.12.07 size 60 and
Grammalecte French Classique 7.7. The French affix header identifies the actual
variant. [Source metadata and retained notices](../crates/vsf-application/assets/dictionaries/README.md)
record exact bytes/hashes and attribution. The English dictionary has multiple
constituent permissive/public-domain terms; its complete notice is retained.
French and the engine use MPL-2.0, with full license text retained. Neither data
nor code is taken from the unverified legacy plugin pack.

## Candidate validation — 2026-10-17

```sh
python3 scripts/verify-language-data.py
cargo run --locked --release -p vsf-application --example language-probe
```

The probe checks actual bundled data for English contractions and French accents,
contractions, ligatures, inflections and decomposed Unicode. It checks expected
suggestions for original typo cases and difficult 32-scalar inputs. Dictionary-wide
ngram fallback is disabled; suggestions are intended to run on demand for the
selected finding, never for every word during document checking.

| Measurement | English | French | Predeclared bound |
| --- | --- | --- | --- |
| Cold initialization | 8.429ms | 15.143ms | 2s off UI thread |
| Selected typo suggestion | 0.215ms | 3.770ms | 250ms |
| Difficult suggestion maximum | 3.018ms | 77.119ms | 250ms |
| 10k repeated-sentence checks | 11.963ms / 70k words | 136.133ms / 50k words | Candidate evidence only |

Logs are retained under ignored `tests/fixtures/generated/language-research/`.
The initial probe failed to compile because the no_std engine parse error does
not implement std::error::Error; explicitly mapping it to a meaningful error
fixes the adapter. No engine/data behavior or performance threshold was changed.

These are microbenchmarks, not the full markup/quality/IPC feature budget. The
full warm 10k analysis/proposal gate remains 500ms p95 /96MiB additional sampled
RSS, with cold initialization <=128MiB for both dictionaries. Ordinary 10k UI
interaction budgets remain unchanged. No Windows/Linux or native language-tool
execution has been established yet.
