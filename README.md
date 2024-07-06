# atomCAD

![CI status](https://github.com/atomCAD/atomCAD/actions/workflows/ci.yml/badge.svg)

A CAD/CAM environment for designing atomically-precise molecular nanotechnology.
Eventually.  Right now it's a decently fast molecular viewer able to parse PDB
files.

![panning around a nanoscale neon pump](./media/neon-pump.gif)

## To Run

1. Install you OS's development tools:
    - __macOS__: `xcode-select --install`
    - __Debian/Ubuntu__: `apt install build-essential`
    - __RHEL/Fedora__: `dnf groupinstall "Development Tools" && dnf install gcc-c++`
    - __Windows__: [Install Visual Studio](https://visualstudio.microsoft.com)
2. [Install Rust](https://rustup.rs/)
3. Install build dependencies:
    - __macOS__: No additional dependencies.
    - __Debian/Ubuntu__: `apt install libx11-dev`
    - __RHEL/Fedora__: `dnf install libX11-devel`
    - __Windows__ 
        - `winget install -e --id Git.Git`
        - Restart your command prompt to apply changes to PATH.
4. `git clone` this repository and navigate to it
5. Run `cargo run`

## Web

If your browser supports WebGPU, you can run atomCAD in your browser:

1. Perform steps 1-3 from the “To Run” section above.
2. Install wasm32 target: `rustup target add wasm32-unknown-unknown`
3. Install trunk: `cargo install --locked trunk`
4. `git clone` this repository and navigate to it
5. Run
    - __macOS/debian/ubuntu__: `RUSTFLAGS="--cfg=web_sys_unstable_apis" trunk serve --open`
    - __Windows__ (PowerShell):
        - `$env:RUSTFLAGS = "--cfg=web_sys_unstable_apis"`
        - `trunk serve --open`
        - (optional) `Remove-Item Env:\RUSTFLAGS`
    - __Windows__ (CMD):
        - `set RUSTFLAGS="--cfg=web_sys_unstable_apis"`
        - `trunk serve --open`
        - (optional) `set RUSTFLAGS=`

## Developers

There is a `check.sh` script which does a similar set of checks as would be
run by the continuous integration checker, but for native and web instead of
all supported platforms.  You will need to download the rust toolchain for
target `wasm32-unknown-unknown` before running this script (see the section
“Web” above).

## License

This project is distributed under the terms of the Mozilla Public License, v.
2.0.  See [LICENSE](./LICENSE) for details.

Some parts of this project are derived from other projects using compatible
licenses, and only those parts are distributed under the terms of their
original license.  See [LICENSE-3RD-PARTY](./LICENSE-3RD-PARTY) or the
copyright declaration of individual files for details.
