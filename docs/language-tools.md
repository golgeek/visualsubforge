# English and French language tools

Task 021 is in progress. Shared analysis, profile/Problems integration, dictionary
suggestions, reviewed exact-range corrections and personal/session words are
implemented with automated coverage. Actual native workflow, project/recovery and
10k native interaction acceptance are still pending; this is not a completed stage.

## Implemented contract

Customize a quality profile to enable English (US) or French spelling and optional
typography, capitalization and existing dialogue-marker checks. All are disabled
in existing profiles. The optional `languageTools` extension retains profile v2;
its omitted default preserves existing serialized policies and fingerprints.
Dictionary/rule choices follow the effective workspace/project profile.

Problems exposes advisory findings, original-source UTF-16 selections and explicit
coverage gaps. Suggestions run only on request for one current spelling finding.
Selecting a suggestion creates a before/after text preview; accepting changes one
occurrence in one Undo step. Timing, order, metadata, tags and surrounding spelling
stay intact. Tag-spanning words can be checked but require manual correction.
Unknown/malformed overrides, karaoke, drawings and cues above 64KiB are skipped
explicitly. SRT includes fansub ASS overrides; ASS/SSA use their native syntax.

Unicode word boundaries join apostrophes/hyphens inside words. Lookup uses NFC and
normalizes curly apostrophes without rewriting source. Engine input is bounded to
32 Unicode scalars/128 UTF-8 bytes. URL/email tokens and unsupported/long words are
reported as skipped. The immutable engine caches use pinned data; dictionary calls
have no internal interruption API. Workers check cancellation before/after selected
suggestions and between bounded tokens, with existing ten-second job deadlines.
Results cap at eight suggestions, 100,000 findings, 100,000 cues and 16MiB source.

Typography offers internal curly apostrophes, exactly-three-dot ellipses, English
punctuation spacing and French nonbreaking spaces before `: ; ? !`. Capitalization
checks the document start and text after `!`/`?`, carrying context across cues;
periods/abbreviations remain deliberately ambiguous. URL punctuation cannot create
a sentence boundary. Dialogue checks normalize an existing leading marker to
`- ` (English) or `— ` (French); they do not infer speakers or change negative numbers.
Every correction is optional and reviewed; this is not full grammar or VSS rule parity.

Personal words persist in user preferences; ignored words last for this app session,
including across document changes, and Problems can clear them. Each list permits
2048 total words across languages, with normalized duplicate detection and the same
word bounds. Settings retains its 256KiB file limit: overflow/failure rejects the
whole save. Persistence succeeds before Rust adopts new personal words. Changes
advance the quality epoch and invalidate reports and open text/timing/statistics
proposals without changing subtitle history. Profiles never contain personal words.
Report `languageCoverage.personalized` identifies user-specific analysis; CLI checks
use the bundled dictionaries without desktop words. Problems, statistics and review
counts capture the same immutable lexicon and source format.

Quality reports use schema 3 with `languageCoverage` and optional per-finding
language details; statistics summaries add whole-document language coverage.
Settings exposes exact dictionary identities, retained full notices and source hashes.

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

## Integration workload declared before measurement

The full gate uses 10,000 cues for each English/French and SRT/ASS/SSA combination,
with all four language rules enabled. The 8,000 checkable cues each include a
unique `FanwordN`, and 2,000 include an additional unique word (10,000 distinct
dictionary misses, avoiding an unrealistically tiny cache),
emoji, decomposed accents, contractions, punctuation, existing dialogue markers,
supported HTML/ASS formatting and deliberately unsupported karaoke/drawing cues.
Measure full quality analysis and an exact single-occurrence correction proposal
(including before/after whole-document analysis), ten timed runs each after warmup.
Also measure cold loading of both dictionaries and bounded selected-word suggestions.
The RSS sampler includes cold dictionaries and subsequent evaluation allocations;
its combined increase must satisfy the stricter 96MiB warm-work budget. Individual
native interaction and lifecycle acceptance remain separate.

## Integration validation — 2026-10-17

```sh
cargo test --locked --workspace
cargo test --locked -p vsf-application --test language
cargo test --locked -p visualsubforge language_ -- --nocapture
cargo test --locked -p vsf-cli --test quality
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
source "$HOME/.nvm/nvm.sh" && nvm use --silent
npm run test
npm run lint
npm run format:check
npm run build
npm run ipc:check
npm run test:browser
python3 scripts/verify-language-data.py
cargo build --locked --release -p vsf-application --example language-benchmark
python3 scripts/language-benchmark.py
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p media-player -p visualsubforge --all-targets --features desktop-e2e,media-prototype -- -D warnings
```

All listed checks pass: 327 ordinary Rust tests, 44 frontend tests, 50 Chromium/WebKit
browser cases, generated IPC, production build and strict static checks. The Rust
total excludes 30 ignored checks; zero-test harnesses are not feature validation.
Nine dedicated application tests cover Unicode/markup/coverage and exact
SRT/ASS/SSA correction/Undo, plus two dictionary tests. Owner tests cover failed
personal writes, state restoration, persisted words/session-only ignores, stale
proposals and the existing 256KiB settings limit. The actual CLI matrix exercises
both dictionaries in all three formats with source preservation and human/JSON
word/range output. Browser Settings has 20 checks; text editing has 10, including
Problems suggestions, exact previews, Undo, ignores and failed personal saves.

The 10k matrix's worst analysis p95 is 74.206958ms and worst correction proposal
p95 150.905625ms (500ms budget). Combined additional sampled RSS, including cold
dictionaries, is 45,514,752 bytes (96MiB budget). Both dictionaries initialize in
25.609541ms; the slowest selected suggestion is 3.691ms. The sampler records
per-phase peaks; this is process RSS, not exhaustive allocation accounting.

Retained failures: the first full matrix reached 612,843,520 additional RSS bytes;
a diagnostic repeat reached 631,537,664. Repeated growth of the dense findings
vector and retaining the first report through the after-report evaluation caused
allocator retention. Bounded preallocation and releasing that report reduced it
to the passing figure above with the same corpus and limits. A first benchmark
compile used non-serializable `Format` instead of its extension string. The initial
correction assertion assumed only one *total* finding disappears, overlooking a
reading-speed change; the corrected test checks exact spelling reduction and the
full before/after reports. The browser's initial ignore check raced the post-Undo
refresh and clicked a disabled control; it now waits for actionable UI and its
acknowledged result. Failed receipts and screenshots remain alongside passing logs
under ignored `tests/fixtures/generated/language-research/`. No product guard or
performance threshold was relaxed.

Native workflow, save/reopen, process-loss recovery, project language policy and
10k native interaction acceptance are the next Task021 work. These automated
results do not establish Windows/Linux execution or physical hardware acceptance.
