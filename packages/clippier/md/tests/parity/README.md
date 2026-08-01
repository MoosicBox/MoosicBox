# Prettier parity harness

`parity.rs` compares `clippier_md` with Prettier 3.8.1 using the Markdown
parser and the repository's `.prettierrc.json`. Oracle invocations use an
explicit nonexistent ignore path so the repository `.prettierignore` cannot
silently echo ignored Markdown input. The pinned CommonMark 0.31.2 checkout
supplies 655 inputs; its HTML output is not used as the oracle.

Initialize the pinned corpus before running parity tests:

```bash
git submodule update --init --recursive -- packages/clippier/md/tests/vendor/commonmark-spec
```

## Focused runs

The harness accepts these environment variables:

| Variable | Meaning | Example |
| --- | --- | --- |
| `CLIPPIER_MD_PARITY_ID` | One CommonMark example ID | `1` |
| `CLIPPIER_MD_PARITY_RANGE` | Inclusive ID range | `1-25` |
| `CLIPPIER_MD_PARITY_SECTION` | Case-insensitive section/subsection match | `Emphasis` |
| `CLIPPIER_MD_PARITY_FIXTURE` | Case-insensitive curated fixture path match | `gfm/table` |
| `CLIPPIER_MD_PARITY_CATEGORY` | Case-insensitive formatter-subsystem match | `whitespace` |
| `CLIPPIER_MD_PARITY_MISMATCH` | Post-evaluation match: `parity`, `idempotence`, `passing`, or mismatch shape | `delimiter-or-escape` |
| `CLIPPIER_MD_PARITY_REPORT` | JSON report path, relative to the workspace root | `target/report.json` |

Filters compose with logical AND. A run fails clearly if no cases match.
For a focused live-oracle run:

```bash
CLIPPIER_MD_PARITY_ID=1 \
  cargo test -p clippier_md --test parity prettier_parity_commonmark_gfm_fixtures -- --nocapture
```

Every run writes a deterministic JSON report containing complete counts,
case metadata, input, expected output, formatter output, second-pass output,
first differing byte/line/column, mismatch shape, strict parity status,
deliberate compatibility-divergence status, and idempotence status. Deliberate
divergences are narrowly shape-checked and remain visible in strict parity
counts; they are accepted only when the formatter output is semantic-preserving
and idempotent. The default report is
`target/clippier-md-parity/latest-report.json`.

## Oracle modes

Set `CLIPPIER_MD_PARITY_ORACLE` to one of:

- `live`: invoke pinned Prettier for every selected case.
- `refresh`: invoke Prettier and write deterministic local cache entries.
- `cache`: use local entries without invoking Prettier; fail if absent or stale.
- `verify`: compare every selected cache entry with an independent live
  Prettier invocation.

When the local cache directory exists, `cache` is the default so ordinary
`cargo test -p clippier_md` runs remain fast. Without a cache, `live` is the
default and still uses pinned Prettier as the authority. Set the mode explicitly
in automation for reproducibility.

The local cache defaults to `target/clippier-md-parity/oracle-v1`. Override it
with `CLIPPIER_MD_PARITY_CACHE_DIR`. Cache keys include the schema,
Prettier version, parser/options, pinned CommonMark revision, virtual path, and
exact input. Cache entries are development artifacts, not an alternate source
of truth.

Generate the complete oracle cache:

```bash
CLIPPIER_MD_PARITY_ORACLE=refresh \
  cargo test -p clippier_md --test parity prettier_parity_commonmark_gfm_fixtures -- --nocapture
```

Run the fast formatter-only corpus check:

```bash
CLIPPIER_MD_PARITY_ORACLE=cache \
  cargo test -p clippier_md --test parity prettier_parity_commonmark_gfm_fixtures -- --nocapture
```

Verify all cached values independently against Prettier 3.8.1:

```bash
CLIPPIER_MD_PARITY_ORACLE=verify \
  cargo test -p clippier_md --test parity prettier_parity_commonmark_gfm_fixtures -- --nocapture
```
