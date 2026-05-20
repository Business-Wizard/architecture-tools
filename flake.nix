{
  description = "Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/2a1d1900c5d04afe589da7ed16111a7418bcaf04"; # 2026-04-23
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config = { allowUnfree = true; };
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        beads = pkgs.callPackage ./nix/beads.nix {};
        loctree = pkgs.callPackage ./nix/loctree.nix {};
        ai-coach = pkgs.callPackage ./nix/ai-coach.nix {};
      in
      {
        packages.ai-coach = ai-coach;
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            jujutsu
            prek
            uv
            rustToolchain
            claude-code
            claude-monitor
            docker
            podman
            jq
            beads
            dolt
          ];
        };
      }
    );
}
