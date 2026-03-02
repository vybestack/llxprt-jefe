# Building and development

This page is for contributors working on `jefe` itself.

## Requirements

- Rust toolchain (edition 2024 crate)
- `tmux` installed and available on PATH
- `llxprt` CLI installed and available on PATH

## Build and run locally

```bash
cargo run
```

Version:

```bash
cargo run -- --version
```

## Development verification

```bash
cargo fmt
cargo check -q
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
```

## Project structure

- `src/main.rs` — app entry + event/render loop wiring
- `src/state/` — app state machine and events
- `src/runtime/` — tmux/PTTY attach, input, snapshots, liveness
- `src/ui/` — screens/components/modals
- `src/theme/` — themes and color resolution
- `src/persistence/` — load/save settings and state
- `docs/` — technical and product docs
