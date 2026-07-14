{ localFlake }:
epkgs:
epkgs.trivialBuild {
  pname = "pedantix";
  postPatch = ''
    substituteInPlace pedantix.el \
      --replace-fail '(defcustom pedantix-program "pedantix"' \
        '(defcustom pedantix-program "${
          localFlake.packages.${epkgs.emacs.stdenv.hostPlatform.system}.pedantix-wrapped
        }/bin/pedantix"'
  '';
  src = ../emacs;
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;
}
