{ localFlake }:
{
  lib,
  config,
  pkgs,
  ...
}:
let
  cfg = config.programs.pedantix;
  tomlFormat = pkgs.formats.toml { };
in
{
  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
    xdg.configFile."pedantix/pedantix.toml" = lib.mkIf (cfg.settings != { }) {
      source = tomlFormat.generate "pedantix.toml" cfg.settings;
    };
  };
  options.programs.pedantix = {
    enable = lib.mkEnableOption "pedantix, the pedantic nix formatter";
    package = lib.mkOption {
      default = localFlake.packages.${pkgs.stdenv.hostPlatform.system}.pedantix-wrapped;
      defaultText = lib.literalExpression "pedantix.packages.\${system}.pedantix-wrapped";
      description = "The pedantix package to install. The default wrapper ships nixfmt, alejandra and nixpkgs-fmt on PATH; use the unwrapped `pedantix` package if you provide the base formatter yourself.";
      type = lib.types.package;
    };
    settings = lib.mkOption {
      inherit (tomlFormat) type;
      default = { };
      description = ''
        Global fallback configuration, using the same structure as pedantix.toml.

        Written to {file}`$XDG_CONFIG_HOME/pedantix/pedantix.toml`.
      '';
      example = lib.literalExpression ''
        {
          preset = "nixos-module";
          lets.sort = true;
        }
      '';
    };
  };
}
