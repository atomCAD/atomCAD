# atomCAD

A CAD environment for designing atomically-precise molecular nanotechnology.

![panning around a nanoscale neon pump](./media/neon-pump.gif)


## To Run

1. Install Rust (https://rustup.rs/)
2. Install build dependencies: `brew install cmake` (macOS), `apt install build-essential cmake libx11-dev` (debian/ubuntu)
3. `git clone` this repository and navigate to it
4. run `cargo run`

## Web

If your browser supports WebGPU, you can run atomCAD in your browser:

1. Install Rust (https://rustup.rs/)
2. Install build dependencies: `brew install cmake` (macOS), `apt install build-essential cmake libx11-dev` (debian/ubuntu)
3. Install wasm32 target: `rustup target add wasm32-unknown-unknown`
4. Install trunk: `cargo install --locked trunk`
5. `git clone` this repository and navigate to it
6. run `RUSTFLAGS=--cfg=web_sys_unstable_apis trunk serve --open`
