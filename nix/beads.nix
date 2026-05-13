{ lib, stdenvNoCC, fetchurl }:

stdenvNoCC.mkDerivation rec {
  pname = "beads";
  version = "1.0.0";

  src = fetchurl {
    url = "https://github.com/steveyegge/beads/releases/download/v${version}/beads_${version}_darwin_arm64.tar.gz";
    hash = "sha256-uHY7Qo5raFUOsrJQVIN5d5S0muSXouJl7Txg8PCgvNI=";
  };

  sourceRoot = "beads_${version}_darwin_arm64";

  installPhase = ''
    mkdir -p $out/bin
    install -m755 bd $out/bin/bd
  '';

  meta = with lib; {
    description = "Distributed, git-backed graph issue tracker for AI agents";
    homepage = "https://github.com/steveyegge/beads";
    license = licenses.mit;
    mainProgram = "bd";
    platforms = [ "aarch64-darwin" ];
  };
}
