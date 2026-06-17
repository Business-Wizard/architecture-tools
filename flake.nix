{
  description = "Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/1f9a45d327c783996acc4690e83ff661fe1cf1b5"; # 2026-05-31
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
        sentrux = pkgs.callPackage ./nix/sentrux.nix {};
        rtk = pkgs.callPackage ./nix/rtk.nix {};
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
            sentrux
            rtk
          ];
        };
      }
    );
}
