{
  description = "A very basic flake";
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = { url = "github:oxalica/rust-overlay"; flake = false; };
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system; overlays = [ (import rust-overlay) ];
        };
        llvmPkgs = pkgs.buildPackages.llvmPackages_11;
        rust = (pkgs.rustChannelOf { date = "2022-02-15"; channel = "nightly"; }).default.override { extensions = [ "rust-src" ]; };
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };
      in
      {
        devShell = (pkgs.mkShell.override { stdenv = llvmPkgs.stdenv; }) {
          nativeBuildInputs = with pkgs; [
            kubectl
            rust
            cargo-edit
            openssl
            pkgconfig
            go-jsonnet
          ];
        };
      }
    );
}
