{ lib, stdenvNoCC, fetchurl }:

stdenvNoCC.mkDerivation rec {
  pname = "sentrux";
  version = "0.5.7";

  binary = fetchurl {
    url = "https://github.com/sentrux/sentrux/releases/download/v${version}/sentrux-darwin-arm64";
    hash = "sha256-MK4aRNRHit8pQBn85tZc5WhsJbyGPrcVsyDFI0kn9sI="; # v0.5.7 — 2026-03-18
  };

  grammars = fetchurl {
    url = "https://github.com/sentrux/sentrux/releases/download/v${version}/grammars-darwin-arm64.tar.gz";
    hash = "sha256-lCoVl/rmwzgj3PcViphQ5LZyWMrfiRAMIMmA/Dq69I8="; # v0.5.7 — 2026-03-18
  };

  dontUnpack = true;

  installPhase = ''
    mkdir -p $out/bin $out/share/sentrux/grammars
    install -m755 $binary $out/bin/sentrux
    tar -xzf $grammars -C $out/share/sentrux/grammars
  '';

  meta = with lib; {
    description = "Architectural sensor for AI-assisted development";
    homepage = "https://github.com/sentrux/sentrux";
    license = licenses.unfree;
    mainProgram = "sentrux";
    platforms = [ "aarch64-darwin" ];
  };
}
