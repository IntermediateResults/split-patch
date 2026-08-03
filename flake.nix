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
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            name = cargoToml.package.name;
            src = ./.;
            version = cargoToml.package.version;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = cargoToml.package.description;
              homepage = cargoToml.package.homepage || cargoToml.package.repository;
            };
          };
        }
      );
    };
}
