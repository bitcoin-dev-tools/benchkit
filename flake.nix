{
  description = "Benchkit shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  outputs = {
    nixpkgs,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem
    (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {inherit system overlays;};
      in
        with pkgs; {
          formatter = alejandra;
          devShells.default = mkShell {
            stdenv = gcc14Stdenv;
            nativeBuildInputs = [
              byacc
              ccache
              cmake
              gcc14
              gnum4
              gnumake
              pkg-config
            ];
            buildInputs = [
              boost
              hwloc
              libevent
              rust-bin.stable.latest.default
              sqlite
              stress
            ];
          };
        }
    );
}
