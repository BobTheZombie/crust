# crust
Meson like build system written in Rust.

## Usage
```bash
crust configure   # Validate the manifest or prepare an external backend
crust build       # Build the project artifacts (native backend by default)
crust test        # Run the project tests (native backend by default)
crust clean       # Clean generated build outputs
```

## Sample project

You can try Crust immediately with the bundled getting-started example:

```bash
crust build \
  --manifest examples/getting-started/crust.build \
  --builddir build/getting-started
./build/getting-started/hello_crust
```

External backends remain available for environments that prefer Ninja or Make:

```bash
crust configure --manifest examples/getting-started/crust.build --backend ninja
(cd build/getting-started && ninja)
```

See `examples/getting-started/README.md` for more details and Make backend
instructions.

## Documentation
- [Build File and Backend Guide](docs/authoring.md)
- [Contributor Guide](docs/contributing.md)
