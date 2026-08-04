# clippier_md

`clippier_md` is a configurable Markdown formatter/checker used by `clippier`.

## Usage

```bash
clippier-md fmt .
clippier-md fmt --check .
clippier-md fmt --check --no-diff .
clippier-md fmt --check --no-diff-cap .
clippier-md fmt --check --color always .
```

## Config

Config is loaded with this precedence:

1. CLI flags
2. `clippier.toml` `[tools.clippier-md]` / `[tools.clippier_md]`
3. `clippier-md.toml`
4. Defaults

Example:

```toml
line-width = 80
trim-trailing-whitespace = true
prose-wrap = "always"
engine = "ast"

[list]
indent-width = 4
style = "preserve"
indentation = "preserve"

[frontmatter]
mode = "preserve"

[headings]
indentation = "preserve"

[files]
respect-gitignore = true
max-concurrency = 0 # 0 derives a bounded default, otherwise sets the worker limit
exclude = ["/generated/**", "/vendor/**"]
skip-dirs = ["node_modules", "target", ".direnv"]

[check.diff]
cap = true
context = 3
max-files = 50
max-lines-per-file = 400
intraline = true
show-invisible-whitespace = true
max-intraline-line-length = 400
```

`files.exclude` patterns are resolved from the directory containing the config
file, independent of the directory where `clippier-md` is invoked. A leading
`/` anchors a pattern to that config directory (not the filesystem root), as in
`/vendor/**`. Config values constructed programmatically use the supplied
working directory as their exclude base.

To preserve authored markdown prose line breaks (similar to Prettier `proseWrap: preserve`), set:

```toml
line-width = 999999

[prose]
wrap = "preserve"
```

In `--check` mode, `clippier-md` prints unified diffs by default.
Use `--no-diff` to disable diff output.
Use `--color auto|always|never` to control ANSI diff colors.
When `show-invisible-whitespace = true`, trailing spaces are shown as `␠`
and tabs as `⇥` on changed lines.

`engine = "ast"` uses markdown AST parsing/printing for robust structure-aware
formatting.

## Configuration behavior

All options below are part of the public formatter contract and are tested
independently of the locked Prettier parity profile:

| Option                        | Values/default                            | Behavior                                                                   |
| ----------------------------- | ----------------------------------------- | -------------------------------------------------------------------------- |
| `line-width`                  | positive integer, default `80`            | Available width for prose wrapping.                                        |
| `trim-trailing-whitespace`    | boolean, default `true`                   | Trims non-fence trailing whitespace while preserving Markdown hard breaks. |
| `end-of-file-newline`         | boolean, default `true`                   | Controls the final newline.                                                |
| `blank-lines.max-consecutive` | non-negative integer, default `1`         | Limits consecutive blank lines outside preserved constructs.               |
| `prose-wrap` / `prose.wrap`   | `always` or `preserve`                    | Reflows prose or retains authored prose line boundaries.                   |
| `engine`                      | `ast` or `legacy`                         | Selects the structural AST formatter or compatibility line formatter.      |
| `list.indent-width`           | positive integer, default `4`             | Sets normalized nested-list indentation.                                   |
| `list.style`                  | `preserve`, `dash`, `plus`, or `asterisk` | Preserves or canonicalizes unordered-list markers.                         |
| `list.indentation`            | `preserve` or `normalize`                 | Preserves authored indentation or applies `indent-width`.                  |
| `frontmatter.mode`            | `preserve` or `normalize`                 | Preserves YAML/TOML bytes or allows normal formatter processing.           |
| `headings.indentation`        | `preserve` or `normalize`                 | Preserves or removes indentation before heading markers.                   |
| `files.respect-gitignore`     | boolean, default `true`                   | Applies nested gitignore rules during discovery.                           |
| `files.exclude`               | glob array                                | Excludes matching paths.                                                   |
| `files.skip-dirs`             | directory-name array                      | Prunes named directories during traversal.                                 |
| `files.max-concurrency`       | integer, default `0`                      | Bounds formatting workers; `0` derives a bounded host default.             |

The AST engine enables GFM and MDX parsing. Unsupported or intentionally
unmodified constructs, including MDX ESM, expressions, and JSX, are preserved
from their source ranges rather than dropped. The legacy engine remains a
separate compatibility path and is not covered by the Prettier corpus claim.
Frontmatter `preserve` mode is immutable; `normalize` currently permits normal
processing but does not promise TOML/YAML semantic reserialization.

The locked parity profile differs from defaults: it uses the AST engine,
`prose-wrap = "always"`, normalized heading/list indentation, and dash list
markers. Behavior under other profiles remains supported and tested, but is not
a byte-for-byte Prettier compatibility claim.

## Prettier compatibility

The AST formatter is tested against Prettier `3.8.1` with its `markdown`
parser, the repository `.prettierrc.json`, and CommonMark `0.31.2` revision
`31c0ca2d294ea60ab4438004da410e2e590a46f2`. The locked corpus contains 655
CommonMark examples plus the curated CommonMark/GFM fixtures under
`packages/clippier/md/tests/parity/fixtures/`.

The compatibility contract is byte-for-byte output equality and formatter
idempotence under the parity profile, with two documented exceptions:
CommonMark examples 440 and 451 expose conflicting, non-idempotent Prettier
forms for emphasized literal `_`. `clippier-md` deliberately canonicalizes both
to the stable, semantic-preserving form `foo *\\_*`. The harness keeps these
visible in strict parity counts and accepts them only when their exact input,
Prettier output, formatter output, and idempotence conditions match.

This is a tested Markdown corpus contract, not a claim of compatibility with
other Prettier parsers, versions, plugins, or unrepresented options.

## Parity fixtures and validation

Tests invoke Prettier at runtime when using the `live`, `refresh`, or `verify`
oracle modes. The runner fallback order is `bunx`, then `pnpm dlx`, then
`npx --yes`.

The harness supports focused ID/range/section/fixture/category runs,
machine-readable reports, deterministic local oracle caching, and independent
live-cache verification. See
[`tests/parity/README.md`](tests/parity/README.md) for exact commands and
environment variables.

When the CommonMark spec submodule is present at
`packages/clippier/md/tests/vendor/commonmark-spec`, parity tests also execute
all examples from `spec.txt` against Prettier.

Initialize/update submodules before running full parity locally:

```bash
git submodule update --init --recursive
```

Frontmatter fixtures are validated separately for byte-for-byte preservation and
are intentionally excluded from live Prettier parity assertions.

Frontmatter (`---` YAML and `+++` TOML) is treated as immutable in preserve
mode and validated byte-for-byte in parity tests.

## Updating the compatibility baseline

Prettier or CommonMark updates are explicit compatibility changes rather than
routine fixture regeneration:

1. Update `PRETTIER_VERSION`, `COMMONMARK_REVISION`, and, when applicable,
   `COMMONMARK_EXAMPLE_COUNT` in `tests/parity.rs`.
2. Checkout the intended CommonMark submodule revision and confirm that the
   parser test reports the expected stable IDs and sections.
3. Remove or use a new local oracle cache directory; never copy old entries to
   satisfy the new cache key.
4. Regenerate the complete oracle with `CLIPPIER_MD_PARITY_ORACLE=refresh`, then
   independently run `CLIPPIER_MD_PARITY_ORACLE=verify`.
5. Run `cargo test -p clippier_md`, workspace formatting checks, and warning-free
   package Clippy. Review every changed output, mismatch classification,
   deliberate divergence, and curated fixture before updating this contract.

Exact commands and report/cache environment variables are documented in
[`tests/parity/README.md`](tests/parity/README.md).
