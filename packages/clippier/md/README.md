# clippier_md

`clippier_md` is a configurable Markdown formatter/checker used by `clippier`.

## Usage

```bash
clippier-md fmt .
clippier-md fmt --check .
clippier-md fmt --check --no-diff .
clippier-md fmt --check --no-diff-cap .
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

[list]
indent-width = 4
style = "preserve"

[frontmatter]
mode = "preserve"

[files]
respect-gitignore = true
exclude = ["**/generated/**", "**/vendor/**"]
skip-dirs = ["node_modules", "target", ".direnv"]

[check.diff]
cap = true
context = 3
max-files = 50
max-lines-per-file = 400
```

In `--check` mode, `clippier-md` prints unified diffs by default.
Use `--no-diff` to disable diff output.
