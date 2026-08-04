# Release CLI verification

Recorded on 2026-04-25 with `target/release/clippier-md`.

- Release-built complete configured repository check: 270 files, no changes,
  exit status 0 after formatting the newly added local benchmark report.
- Both large plans pass explicit clean checks with exit status 0.
- Three repeated parallel JSON checks produce identical ordered summaries.
- A controlled changed file returns status 1 in check mode and remains unmodified.
- No-diff, capped text diff, uncapped JSON diff, and write modes all succeed with
  their expected output shape.
- Write mode selects only the requested file and produces exactly:

```markdown
one two
three four
```

No persistent cache exists, so cold and warm invocations use the same formatter
path and semantics.
