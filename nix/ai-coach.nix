{ lib, buildNpmPackage, fetchFromGitHub, nodejs_22 }:

buildNpmPackage rec {
  pname = "ai-engineer-coach";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "microsoft";
    repo = "AI-Engineering-Coach";
    rev = "a95f1262da1f71a8f6769eeb5d161c1964985e58";
    hash = "sha256-TKO1zvbYBOwT9HHWBOpGzJOHGBtTfIIHUIR6xW6EI4c=";
  };

  nodejs = nodejs_22;

  npmDepsHash = "sha256-mguRARmufRghI5sFpIa0kNA2vSVha2ha+RR855yndck=";

  # keytar's C++ addon fails on modern clang; VS Code provides it at runtime
  npmFlags = [ "--ignore-scripts" ];

  buildPhase = ''
    npm run package
  '';

  installPhase = ''
    mkdir -p $out
    cp ai-engineer-coach-*.vsix $out/
  '';

  meta = with lib; {
    description = "VS Code extension for AI engineering coaching and session analysis";
    homepage = "https://github.com/microsoft/AI-Engineering-Coach";
    license = licenses.mit;
    platforms = platforms.all;
  };
}
