{ localFlake }:
{
  lib,
  config,
  pkgs,
  ...
}:
let
  cfg = config.programs.pedantix;
  flattenSettings =
    prefix: attrs:
    lib.concatLists (
      lib.mapAttrsToList (
        name: value:
        let
          key = if prefix == "" then tomlKey name else "${prefix}.${tomlKey name}";
        in
        if lib.isAttrs value && value != { } then
          flattenSettings key value
        else
          [ "${key}=${toTomlValue value}" ]
      ) attrs
    );
  settingsArgs = lib.concatMap (kv: [
    "--set"
    kv
  ]) (flattenSettings "" cfg.settings);
  toTomlValue =
    value:
    if lib.isAttrs value then
      "{ ${
        lib.concatStringsSep ", " (lib.mapAttrsToList (k: v: "${tomlKey k} = ${toTomlValue v}") value)
      } }"
    else if lib.isList value then
      "[${lib.concatStringsSep ", " (map toTomlValue value)}]"
    else if lib.isString value || lib.isBool value || lib.isInt value || lib.isFloat value then
      builtins.toJSON value
    else
      throw "programs.pedantix.settings: unsupported value ${lib.generators.toPretty { } value}";
  tomlKey = key: if builtins.match "[A-Za-z0-9_-]+" key != null then key else builtins.toJSON key;
in
{
  config = lib.mkIf cfg.enable {
    settings.formatter.pedantix = {
      inherit (cfg) includes;
      command = lib.getExe cfg.package;
      options =
        lib.optionals (cfg.configFile != null) [
          "--config"
          (toString cfg.configFile)
        ]
        ++ settingsArgs
        ++ cfg.extraArgs;
    }
    // lib.optionalAttrs (cfg.excludes != [ ]) { inherit (cfg) excludes; }
    // lib.optionalAttrs (cfg.priority != null) { inherit (cfg) priority; };
  };
  options.programs.pedantix = {
    enable = lib.mkEnableOption "pedantix, the pedantic Nix formatter";
    package = lib.mkOption {
      default = localFlake.packages.${pkgs.stdenv.hostPlatform.system}.pedantix-wrapped;
      defaultText = lib.literalExpression "pedantix.packages.\${system}.pedantix-wrapped";
      description = "The pedantix package to run. The default wrapper ships nixfmt, alejandra and nixpkgs-fmt on PATH; use the unwrapped `pedantix` package if you provide the base formatter yourself.";
      type = lib.types.package;
    };
    configFile = lib.mkOption {
      default = null;
      description = "A complete pedantix.toml to use instead of config file discovery (`settings` still applies on top).";
      type = lib.types.nullOr lib.types.path;
    };
    excludes = lib.mkOption {
      default = [ ];
      description = "Path / file patterns to exclude.";
      example = [ "generated/*.nix" ];
      type = lib.types.listOf lib.types.str;
    };
    extraArgs = lib.mkOption {
      default = [ ];
      description = "Extra command line arguments passed to pedantix, appended after the flags generated from `settings` and `configFile`.";
      example = [ "--formatter=nixpkgs-fmt" ];
      type = lib.types.listOf lib.types.str;
    };
    # Declared for compatibility with treefmt-nix's build.programs, which
    # probes options.programs.*.finalPackage on every enabled program; left
    # undefined so treefmt falls back to `package` (mirrors mkFormatterModule).
    finalPackage = lib.mkOption {
      description = "Resulting pedantix package.";
      readOnly = true;
      type = lib.types.package;
    };
    includes = lib.mkOption {
      default = [ "*.nix" ];
      description = "Path / file patterns to include.";
      type = lib.types.listOf lib.types.str;
    };
    priority = lib.mkOption {
      default = null;
      description = "treefmt priority, for ordering relative to other formatters on the same files.";
      type = lib.types.nullOr lib.types.int;
    };
    settings = lib.mkOption {
      default = { };
      description = ''
        pedantix configuration, using the same structure as pedantix.toml.

        Passed as `--set` flags, i.e. each key is merged on top of any pedantix.toml discovered in the project (`overrides` replaces a discovered override list; preset overrides still combine).
      '';
      example = lib.literalExpression ''
        {
          preset = "nixos-module";
          formatter = "alejandra";
          args.first = [ "self" "lib" "config" "pkgs" ];
          lets.sort = true;
          overrides = [
            {
              path = "**.programs.git.settings.alias";
              attrs.sort = false;
            }
          ];
        }
      '';
      type = lib.types.attrsOf lib.types.anything;
    };
  };
}
