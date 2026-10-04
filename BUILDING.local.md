# Reproducible Ubuntu build

Use the pinned Rust toolchain. Run `cargo fetch --locked` once to populate the dependency cache before offline tests. Keep CARGO_BUILD_JOBS=2 when building alongside other workspaces.

```sh
cargo test --locked --offline --workspace
```

