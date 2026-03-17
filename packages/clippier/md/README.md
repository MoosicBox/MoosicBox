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
exclude = ["**/generated/**", "**/vendor/**"]
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

## Parity Fixtures

`packages/clippier/md/tests/parity/fixtures/` contains parity source fixtures for
CommonMark + GFM behavior. Tests run `prettier` (pinned to `3.8.1`) at runtime
and compare output byte-for-byte with `clippier-md`.

Frontmatter (`---` YAML and `+++` TOML) is treated as immutable in preserve
mode and validated byte-for-byte in parity tests.
