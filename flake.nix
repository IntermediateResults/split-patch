{
  description = "A flake for developing and building split-patch";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      rustVersion = "latest";
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "cargo"
              "rustfmt"
              "clippy"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
            ];
          };
        }
      );

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            name = "split-patch";
            src = ./.;
            version = self.rev or self.dirtyShortRev;
            cargoLock.lockFile = ./Cargo.lock;
          };
        }
      );
    };
}
