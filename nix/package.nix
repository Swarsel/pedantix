{
  lib,
  alejandra,
  nixfmt,
  nixpkgs-fmt,
  rustPlatform,
  writeShellApplication,
}:
let
  pedantix = rustPlatform.buildRustPackage {
    cargoLock.lockFile = ../Cargo.lock;
    meta = {
      description = "Pedantic Nix formatter with deterministic attribute and argument ordering";
      license = lib.licenses.mit;
      mainProgram = "pedantix";
    };
    nativeCheckInputs = [
      alejandra
      nixfmt
      nixpkgs-fmt
    ];
    passthru.wrapped = wrapped;
    pname = "pedantix";
    src = lib.fileset.toSource {
      fileset = lib.fileset.unions [
        ../Cargo.toml
        ../Cargo.lock
        ../src
        ../presets
        ../tests
        ../example.nix
        ../pedantix.toml
      ];
      root = ../.;
    };
    version = (lib.importTOML ../Cargo.toml).package.version;
  };
  wrapped = writeShellApplication {
    name = "pedantix";
    runtimeInputs = [
      pedantix
      alejandra
      nixfmt
      nixpkgs-fmt
    ];
    text = ''exec pedantix "$@"'';
  };
in
pedantix
