# UUID retirement verification

Tested on aarch64 Darwin, Nix Rust 1.99.0. The test directory was an archive of `57cf1627f42d` with the changed Switchy sources overlaid. Only the disposable copy's lock was refreshed. Consumer integration must refresh the `switchy_uuid` lock entry (new normal edge `switchy_random`, dev edges `serde_json` and registry `bmux_codec`); no new root workspace dependency is required.

`tests/contract.tsv` is an external oracle, not candidate output. Each row contains the exact input (including whitespace), followed by `!` for rejection or simple hex, JSON, and stable BMUX binary hex. Captured with `uuid::Uuid::parse_str`, `Uuid::simple`, `serde_json::to_string`, and `bmux_codec::to_vec` on the **registry type**. The 67 rows from locked 1.29.0 and BMUX's 1.29.3 compared byte-for-byte equal (22 accepted, 45 rejected). SHA-256: `3244b7dcef1c0fa9cbbe57de3e472efdebf0b538d9463fae9d452f1a3b698803`. Separate seed-42 UUIDs were captured from the unmodified Switchy simulator at `57cf1627f42d`.

Commands inside `nix develop --offline --no-write-lock-file`, in the disposable workspace:

```sh
for f in serde native,serde simulator,serde uuid,simulator,serde; do
  cargo test --locked --offline -p switchy_uuid --no-default-features --features "$f" --lib --test contract
done
cargo test --locked --offline -p switchy_uuid --no-default-features --features native,serde --doc
cargo test --locked --offline -p switchy_random --lib secure_tests
cargo tree --locked --offline -e normal -p switchy_uuid --no-default-features --features native,simulator,serde
cargo clippy --locked --offline -p switchy_uuid --features serde --lib --test contract -- -D warnings
cargo clippy --locked --offline -p switchy_uuid --no-default-features --features native,serde --lib --test contract -- -D warnings
cargo clippy --locked --offline -p switchy_random --lib -- -D warnings
export RUSTFLAGS='--cfg tokio_unstable --cfg getrandom_backend="custom"'
for f in rand rand,simulator; do
  cargo test --locked --offline -p switchy_random --no-default-features --features "$f" --lib secure_tests::intercepted_os_entropy_fails_before_writing -- --exact
done
```

Result: respectively 18/22/34/39 UUID unit tests and two contract tests per configuration, 24 doctests, three ordinary entropy tests, both custom entropy tests, and all listed Clippy checks passed. The normal dependency tree for the owned configuration contains no `uuid` crate. This does not retire direct dependencies in other packages or prove cross-target support. `uuid-compat` still needs the external type; no-default users of conversions must request it. `switchy_random`'s pre-existing historical general-purpose replay failure is separate from the now-frozen UUID replay.
