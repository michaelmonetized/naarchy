# Contributing

The binary is the contract. If SPEC and the code disagree, the code is right
and SPEC is late — fix SPEC in the same change.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --bins
bash scripts/smoke.sh
```

Do not add a Settings GUI, a user CSS file, `sha2`, telemetry, or an IPC reply
protocol without a design pass. v0.2 lives in SPEC §8.

Voice in user-facing docs: terse, solution first, no pitch.
