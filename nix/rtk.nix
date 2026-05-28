{ lib, stdenvNoCC, fetchurl }:

stdenvNoCC.mkDerivation rec {
  pname = "rtk";
  version = "0.42.0";

  src = fetchurl {
    url = "https://github.com/rtk-ai/rtk/releases/download/v${version}/rtk-aarch64-apple-darwin.tar.gz";
    hash = "sha256-zdyc0RzfgLM0LuuroOarJtnI3sRSlepEz5gGKYcYVyQ=";
  };

  unpackPhase = ''
    tar -xzf $src
  '';

  installPhase = ''
    mkdir -p $out/bin
    install -m755 rtk $out/bin/rtk
  '';

  meta = with lib; {
    description = "CLI proxy that reduces LLM token consumption by compressing command output";
    homepage = "https://github.com/rtk-ai/rtk";
    license = licenses.mit;
    mainProgram = "rtk";
    platforms = [ "aarch64-darwin" ];
  };
}
