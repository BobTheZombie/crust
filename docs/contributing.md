# Contributor Guide

This guide explains coding conventions, where modules live, and how to extend Crust
with new backends or language handlers.

## Coding standards

- Follow standard Rust style and run `cargo fmt` before submitting changes.
- Prefer error handling with `anyhow` and `Context` to provide actionable messages.
- Keep functions small and focused; share logic across modules instead of duplicating
  backend-specific code.
- Add unit tests for new behavior in the module you touch and ensure `cargo test`
  passes locally.

## Module layout

- `src/main.rs` houses the CLI, argument parsing, backend selection, and commands such
  as `configure`, `build`, `test`, and `clean`.
- `src/config/` defines the TOML manifest structures (`ProjectManifest`, `Target`) and
  contains parsing tests.
- `src/graph/` builds the dependency graph, validates references, checks for cycles,
  and performs incremental/out-of-date detection.
- `src/backend/` contains backend implementations. The shared `Backend` trait declares
  the `name` and `emit` methods, and each backend writes its generated files to the
  requested build directory.

## Adding a backend

1. Create a new module under `src/backend/` and implement the `Backend` trait.
2. Export the backend from `src/backend/mod.rs` and add it to the selection logic in
   `src/main.rs` so the CLI can instantiate it.
3. Decide what file(s) your backend should emit (for example `build.ninja` or
   `Makefile`) and return them via `BackendEmitResult`.
4. Add unit tests for the backend module to validate emitted contents.

## Adding language handlers or target types

1. Extend `Target` in `src/config/mod.rs` with the new variant and serialization rules.
2. Update `DependencyGraph` to map the variant to the appropriate `TargetKind`, sources,
   outputs, and optional command data.
3. Teach each backend how to emit the new variant. For compiled languages this may
   include writing new rules or invoking toolchain commands; for generators it may
   involve custom command wiring.
4. Add tests that parse the new manifest shape and ensure backends render the expected
   outputs.

## Documentation and examples

- Update `docs/` with any new behavior and provide minimal runnable examples in
  `examples/` when adding a new target type or backend feature.
- Keep README usage in sync with CLI changes and add troubleshooting notes when you fix
  common pitfalls.
