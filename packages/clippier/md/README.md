# clippier_md

`clippier_md` is a configurable Markdown formatter/checker used by `clippier`.

## Usage

```bash
clippier-md fmt .
clippier-md fmt --check .
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
```
