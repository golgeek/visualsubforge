# Bundled dictionary sources

These files are unmodified data from LibreOffice dictionaries commit
`67032ee957631908098a72edf48dc81cac27e369`. Exact source URLs, byte counts and
SHA256 values are recorded in `sources.json`.

- **English (United States):** SCOWL 2020.12.07, size 60, `en/en_US.aff` and
  `en/en_US.dic`. The full `README-en-US.txt` preserves SCOWL, Ispell, WordNet,
  VarCon and constituent source notices/permissions. Do not replace them with
  a guessed single license label.
- **French (Classique):** Grammalecte 7.7, `fr_FR/dictionaries/fr.aff` and `fr.dic`.
  The actual affix header identifies Classique. `README-fr.txt` retains all
  upstream attribution and the MPL-2.0 declaration. `MPL-2.0.txt` supplies the
  full license, copied unchanged from the pinned Spellbook 0.4.2 source archive.

The candidate engine is Spellbook 0.4.2 (MPL-2.0); `sources.json` identifies its
published crate checksum. No external dictionary parser, download or executable
is run at application startup. No legacy plugin payload is included here.

Run `python3 scripts/verify-language-data.py` from the repository root after any
intentional data update. Updates must review the new upstream notices, retain
exact source identity, and rerun language behavior/performance tests. Current
feature integration status is recorded in [language tools](../../../../docs/language-tools.md).
