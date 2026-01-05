# Getting started sample project

This example shows a minimal `crust.build` manifest and the commands needed to
configure and build it.

## Project layout

```
examples/getting-started/
├── crust.build   # Manifest with a single executable target
└── src/
    └── main.c    # Simple C program built by crust
```

## Build steps

1. Configure the build with your preferred backend (Ninja shown here):
   ```bash
   crust configure \
     --manifest examples/getting-started/crust.build \
     --builddir build/getting-started \
     --backend ninja
   ```
2. Run the backend from inside the build directory to compile the sample:
   ```bash
   (cd build/getting-started && ninja)
   ```
3. Execute the resulting binary:
   ```bash
   ./build/getting-started/hello_crust
   ```

You can switch to the Make backend by passing `--backend make` during the
configure step and running `make` from the build directory instead of `ninja`.
