{ lib, stdenv, python3Packages, fetchPypi, autoPatchelfHook, zlib }:

let
  platformInfos = {
    "x86_64-linux" = {
      platform = "manylinux1_x86_64";
      hash = "sha256-oUL7EVbfwgV1K1Rv0kzW5r1AXr167BCXwzntDgVyTc0=";
    };
    "x86_64-darwin" = {
      platform = "macosx_10_9_x86_64";
      hash = "sha256-Ynn1BUBrmzRV2sL5ZGwOEQ/ccoV0edwFt4iiz9KN+k8=";
    };
    "aarch64-darwin" = {
      platform = "macosx_11_0_arm64";
      hash = "sha256-iQNOSoAClAk2FMjAExfgsFHDXS56vwieePGDCYRRbgQ=";
    };
  };

  inherit (stdenv.hostPlatform) system;
  platformInfo = platformInfos.${system} or (throw "Unsupported platform ${system}");
in
python3Packages.buildPythonApplication (finalAttrs: {
  pname = "sourcery";
  version = "1.43.0";
  format = "wheel";

  src = fetchPypi {
    inherit (finalAttrs) pname version;
    format = "wheel";
    inherit (platformInfo) platform hash;
    python = "py2.py3";
    abi = "none";
  };

  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = [ zlib ];

  meta = {
    description = "AI-powered code review and pair programming tool for Python";
    homepage = "https://sourcery.ai";
    license = lib.licenses.unfree;
    mainProgram = "sourcery";
    platforms = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" ];
  };
})
