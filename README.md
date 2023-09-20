# atomCAD

![CI status](https://github.com/atomCAD/atomCAD/actions/workflows/ci.yml/badge.svg)

A CAD environment for designing atomically-precise molecular nanotechnology.

![panning around a nanoscale neon pump](./media/neon-pump.gif)

## To Run

1. [Install Rust](https://rustup.rs/)
2. Install build dependencies:
   - __Debian/Ubuntu__: `apt install build-essential libx11-dev`
   - __RHEL/Fedora__: `dnf groupinstall "Development Tools" && dnf install gcc-c++ libX11-devel`
   - __Windows__
     - [Install Git](https://git-scm.com/download/win)
3. `git clone` this repository and navigate to it
4. run `cargo run`

## Web

atomCAD supports running in a browser:

1. [Install Rust](https://rustup.rs/)
2. Install build dependencies:
   - __Debian/Ubuntu__: `apt install build-essential libx11-dev`
   - __RHEL/Fedora__: `dnf groupinstall "Development Tools" && dnf install gcc-c++ libX11-devel`
   - __Windows__
     - [Install Git](https://git-scm.com/download/win)
3. Install wasm32 target: `rustup target add wasm32-unknown-unknown`
4. Install trunk: `cargo install --locked trunk`
5. `git clone` this repository and navigate to it
6. Run `trunk serve --open`

## Developers

There is a `check.sh` script which does a similar set of checks as would be
run by the continuous integration checker, for your platform and web.  The
continuous integration checker runs these checks for all supported platforms,
so don't be surprised if some checks fail when you make a PR against the base
repository.

## License

This project is distributed under the terms of the Mozilla Public License, v.
2.0.  See [LICENSE](./LICENSE) for details.

Some parts of this project are derived from other projects using compatible
licenses, and only those parts are distributed under the terms of their
original license.  See [CREDITS](credits/CREDITS.md) or the copyright
declaration of individual files for details.
