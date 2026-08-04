# Full parity verification

Recorded on 2026-04-25 against Prettier 3.8.1 and the pinned CommonMark corpus.

```text
live:   677/679 strict parity, 2/2 documented deliberate divergences,
        0 idempotence failures, 252,991 ms harness runtime
verify: 677/679 strict parity, 2/2 documented deliberate divergences,
        0 idempotence failures, 242,089 ms harness runtime
```

Commands:

```bash
CLIPPIER_MD_PARITY_ORACLE=live \
  cargo test -p clippier_md --test parity \
  prettier_parity_commonmark_gfm_fixtures -- --nocapture

CLIPPIER_MD_PARITY_ORACLE=verify \
  cargo test -p clippier_md --test parity \
  prettier_parity_commonmark_gfm_fixtures -- --nocapture
```

Oracle refresh was intentionally not run because formatter optimization changed
neither corpus inputs nor expected Prettier output. The two divergences remain
the pre-existing, exact CommonMark examples 440 and 451 literal-underscore
compatibility cases documented by the harness.
