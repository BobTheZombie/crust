# Build File and Backend Guide

This guide explains how to author `crust.build` files, what targets are supported,
how toolchains are selected, and what each backend emits. It also includes quickstart
examples and troubleshooting steps to help you iterate quickly.

## Build file syntax

Crust uses a TOML manifest called `crust.build` with two main sections:

- `[project]` table defines the project name and optional `version` string.
- `[[targets]]` array of tables declares each build target. All target names must be
  unique within a manifest, and dependencies must reference other declared targets.

Common target keys:

- `type` (required): one of `executable`, `static_library`, `shared_library`, or
  `custom_command`.
- `name` (required): logical target name; it also controls generated output names.
- `sources` (required for compiled targets): source file list relative to the manifest
  directory. Custom commands use `inputs` instead of `sources`.
- `deps` (optional): other targets this entry depends on.
- `command` and `outputs` (custom commands only): command string to run and the files
  it should produce.

Example manifest outline:

```toml
[project]
name = "hello"
version = "0.1.0"

[[targets]]
type = "executable"
name = "hello"
sources = ["src/main.c"]
deps = ["util"]

[[targets]]
type = "static_library"
name = "util"
sources = ["src/util.c"]
```

## Supported target types

Crust currently understands four target kinds:

- **Executable**: produces a binary named after the target (`hello`).
- **Static library**: archives sources into `lib<name>.a` (for example, `libutil.a`).
- **Shared library**: links sources into `lib<name>.so` (for example, `libmath.so`).
- **Custom command**: runs an arbitrary `command`, treating `inputs` as sources and
  `outputs` as generated artifacts. Other targets can depend on the custom command by
  listing its `name` in their `deps`.

Crust validates that dependencies exist, rejects duplicate target names, and computes
an incremental dependency graph so backends can emit rules in topological order.

## Toolchain selection and detection

The CLI lets you pick a backend with `--backend ninja` or `--backend make`.
Crust does not auto-probe toolchains today; it assumes the selected backend binary
(`ninja` or `make`) is available in your `PATH`. The configuration step checks whether
previous backend outputs are older than the manifest or any listed sources and
regenerates files when needed, so you can re-run `crust configure` safely.

## Backend output

Backends are responsible for translating the dependency graph into concrete build
files under the chosen build directory (defaults to `build/`):

- **Ninja backend** emits `build.ninja` with simple stamp rules for each target. It
  wires sources and dependent outputs into each rule and sets `builddir` and `srcdir`
  variables at the top of the file.
- **Make backend** emits a `Makefile` that touches outputs by default or runs the
  provided custom command. It defines `SRCROOT` and `BUILDDIR` variables and writes one
  rule per target output.

When `crust build` or `crust test` is invoked, the CLI prints a hint that shows which
command to run (`ninja` or `make`) from inside the build directory.

## Quickstart examples

You can try Crust with the bundled examples:

1. Configure a build:
   ```bash
   crust configure --manifest examples/hello/crust.build --builddir build/hello --backend ninja
   ```
2. Run the suggested backend in the build directory:
   ```bash
   (cd build/hello && ninja)
   ```
3. Explore other manifests such as `examples/library/crust.build` or
   `examples/custom/crust.build` to see shared libraries and custom commands.

## Troubleshooting

- **Unknown dependency or duplicate target**: ensure every name in `deps` matches a
  declared target and that target names are unique.
- **Backend not regenerating**: Crust compares manifest and source modification times
  against backend output files; touch or update sources and re-run `crust configure`
  if changes were missed.
- **Backend command missing**: install the chosen backend (`ninja` or `make`) and make
  sure it is available on your `PATH`.
- **Generated files missing**: confirm custom commands declare correct `outputs` and
  that downstream targets depend on the custom command by name.
