{ lib, rustPlatform, fetchFromGitHub, pkg-config, openssl }:

rustPlatform.buildRustPackage rec {
  pname = "loctree";
  version = "0.8.16";

  src = fetchFromGitHub {
    owner = "Loctree";
    repo = "Loctree";
    rev = "v${version}";
    hash = "sha256-GcIokw/pxvY78oElDgGZcSl0LugNi6Ah0de+Ktx+1B0=";
  };

  cargoHash = "sha256-vlExS0m4XYZrzmO0uu4Ra0D/iHglC2EoJb7LbMgeWYo=";

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [ openssl ];

  # Build both CLI and MCP server binaries
  cargoBuildFlags = [ "--bin" "loctree" "--bin" "loctree-mcp" ];

  # Tests may require network or git (common in Rust projects)
  doCheck = false;

  # Create 'loct' symlink for MCP compatibility (loct is the new name in v0.9.0+)
  postInstall = ''
    ln -s $out/bin/loctree $out/bin/loct
  '';

  meta = with lib; {
    description = "Holographic code map for AI agents - dependency graphs, dead code detection, impact analysis";
    homepage = "https://loctree.io/";
    license = with licenses; [ mit asl20 ];  # Dual-licensed MIT OR Apache-2.0
    mainProgram = "loctree";
  };
}
