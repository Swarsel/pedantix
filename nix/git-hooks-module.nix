{ localFlake }:
{ lib, pkgs, ... }:
{
  hooks.pedantix = {
    description = lib.mkDefault "Check that Nix files are formatted with pedantix";
    entry = lib.mkDefault "${
      lib.getExe localFlake.packages.${pkgs.stdenv.hostPlatform.system}.pedantix-wrapped
    } --check";
    files = lib.mkDefault "\\.nix$";
  };
}
