{ pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/archive/8f6cd53206e2d4cc783a7df6f72d311ffc544c8f.tar.gz") {} }:

pkgs.mkShell {
  packages = with pkgs; [
    rustup
    cmake
    libiconv
  ];
}
