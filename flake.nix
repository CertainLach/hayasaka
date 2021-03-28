{
  description = "A very basic flake";
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs-mozilla = { url = github:mozilla/nixpkgs-mozilla; flake = false; };
    cargo2nix.url = github:onsails/cargo2nix/flake;
  };
  outputs = { self, nixpkgs, flake-utils, nixpkgs-mozilla, cargo2nix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        rustChannel = "1.51.0";
        rustChannelSha256 = "sha256-+EFKtTDUlFY0aUXdSvrz7tAhf5/GsqyVOp8skXGTEJM=";
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import "${nixpkgs-mozilla}/rust-overlay.nix")
            cargo2nix.overlay
          ];
        };
        rustPkgs = pkgs.rustBuilder.makePackageSet' {
          inherit rustChannel rustChannelSha256;
          packageFun = import ./Cargo.nix;
          localPatterns = [ ''^(src|crates)(/.*)?'' ''[^/]*\.(rs|toml)$'' ];
        };

        hayasaka = rustPkgs.workspace.hayasaka { };
      in
      {
        defaultPackage = hayasaka;

        devShell = pkgs.mkShell {
          nativeBuildInputs = [
            pkgs.binutils
            pkgs.pkgconfig
            pkgs.openssl
            pkgs.kubernetes-helm
            cargo2nix.packages.${system}.cargo2nix
            (pkgs.rustChannelOf {
              channel = rustChannel;
              sha256 = rustChannelSha256;
            }).rust
          ];
        };

        legacyPackages.image = pkgs.dockerTools.buildImage {
          name = "hayasaka";
          tag = "latest";
          contents = [
            hayasaka
            pkgs.bashInteractive
            pkgs.coreutils
            pkgs.kubernetes-helm
            pkgs.jsonnet-bundler
          ];
          config = {
            Cmd = [
              "bash"
            ];
          };
        };
      }
    );
}
