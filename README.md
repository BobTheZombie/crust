# crust
Meson like build system written in Rust.

## Usage
```bash
crust configure   # Configure the project before building
crust build       # Build the project artifacts
crust test        # Run the project tests
crust clean       # Clean generated build outputs
```

## Sample project

You can try Crust immediately with the bundled getting-started example:

```bash
crust configure \
  --manifest examples/getting-started/crust.build \
  --builddir build/getting-started \
  --backend ninja
(cd build/getting-started && ninja)
./build/getting-started/hello_crust
```

See `examples/getting-started/README.md` for more details and Make backend
instructions.

## Documentation
- [Build File and Backend Guide](docs/authoring.md)
- [Contributor Guide](docs/contributing.md)
