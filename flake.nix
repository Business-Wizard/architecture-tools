{
  description = "Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/2a1d1900c5d04afe589da7ed16111a7418bcaf04"; # 2026-04-23
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config = { allowUnfree = true; };
        };
        beads = pkgs.callPackage ./nix/beads.nix {};
        loctree = pkgs.callPackage ./nix/loctree.nix {};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            jujutsu
            prek
            uv
            cargo
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
