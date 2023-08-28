# atomCAD

A CAD/CAM environment for designing atomically-precise molecular nanotechnology.
Eventually.  Right now it's a decently fast molecular viewer able to parse PDB
files.

![panning around a nanoscale neon pump](./media/neon-pump.gif)

## To Run

1. [Install Rust](https://rustup.rs/)
2. Install build dependencies: 
    - __macOS__: `brew install cmake`
    - __debian/ubuntu__: `apt install build-essential cmake libx11-dev` 
    - __Windows__ 
        - [Install CMake](https://cmake.org/download/)
        - [Install Ninja](https://ninja-build.org/) - manual setup, ensure it's in your PATH
3. `git clone` this repository and navigate to it
4. run `cargo run`

## Web

If your browser supports WebGPU, you can run atomCAD in your browser:

1. [Install Rust](https://rustup.rs/)
2. Install build dependencies: 
    - __macOS__: `brew install cmake`
    - __debian/ubuntu__: `apt install build-essential cmake libx11-dev` 
    - __Windows__ 
        - [Install CMake](https://cmake.org/download/)
        - [Install Ninja](https://ninja-build.org/) - manual setup, ensure it's in your PATH
3. Install wasm32 target: `rustup target add wasm32-unknown-unknown`
4. Install trunk: `cargo install --locked trunk`
5. `git clone` this repository and navigate to it
6. Run
    - __macOS/debian/ubuntu__  `RUSTFLAGS=--cfg=web_sys_unstable_apis trunk serve --open`
    - __Windows__ set env variable `set "RUSTFLAGS=--cfg=web_sys_unstable_apis"` and then execute  `trunk serve --open`

## Developers

There is a `check.sh` script which does a similar set of checks as would be
run by the continuous integration checker, for all supported platforms.  You
will need to download the rust toolchain for each supported build target
before running this script:

```bash
rustup target add aarch64-linux-android thumbv7neon-linux-androideabi \
    x86_64-linux-android i686-linux-android aarch64-apple-ios \
    aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin \
    x86_64-apple-darwin x86_64-unknown-linux-gnu i686-unknown-linux-gnu \
    riscv64gc-unknown-linux-gnu aarch64-unknown-linux-gnu \
    thumbv7neon-unknown-linux-gnueabihf powerpc64-unknown-linux-gnu \
    powerpc64le-unknown-linux-gnu x86_64-pc-windows-msvc x86_64-pc-windows-gnu \
    i686-pc-windows-msvc i686-pc-windows-gnu aarch64-pc-windows-msvc \
    wasm32-unknown-unknown
```

## License

This project is distributed under the terms of the Mozilla Public License, v.
2.0.  See [LICENSE](./LICENSE) for details.

Some parts of this project are derived from other projects using compatible
licenses, and only those parts are distributed under the terms of their
original license.  See [LICENSE-3RD-PARTY](./LICENSE-3RD-PARTY) or the
copyright declaration of individual files for details.
