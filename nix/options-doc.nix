{
  pkgs,
  localFlake,
}:
let
  inherit (pkgs) lib;

  # Render the `programs.pedantix.*` options of a module to CommonMark, straight
  # from its `mkOption` declarations, so the docs cannot drift from the module.
  optionsDoc =
    {
      module,
      extraModules ? [ ],
    }:
    let
      doc = pkgs.nixosOptionsDoc {
        # Only document the module's own `programs.pedantix.*` options, not the
        # stub options declared above to satisfy evalModules.
        options = {
          programs.pedantix = eval.options.programs.pedantix;
        };
        transformOptions =
          opt:
          opt
          // {
            # Strip the declaration path; it points into the nix store and is
            # noise in the rendered reference.
            declarations = [ ];
            # `finalPackage` is an internal treefmt-nix compatibility shim, not
            # a user-facing option.
            visible = if lib.elem "finalPackage" opt.loc then false else opt.visible;
          };
        warningsAreErrors = false;
      };
      eval = lib.evalModules {
        modules = [
          (module { inherit localFlake; })
          { _module.args = { inherit pkgs; }; }
        ]
        ++ extraModules;
      };
    in
    # Shift each option heading one level deeper (`##` -> `###`) so it nests
    # under the `## Options` heading of the page that includes it.
    pkgs.runCommand "pedantix-options.md" { } ''
      sed 's/^## /### /' ${doc.optionsCommonMark} > "$out"
    '';
in
{
  hm = optionsDoc {
    # Stub the home-manager options the module's `config` block writes to, so
    # evalModules can merge it without pulling in all of home-manager.
    extraModules = [
      {
        options = {
          home.packages = lib.mkOption {
            default = [ ];
            type = lib.types.listOf lib.types.package;
          };
          xdg.configFile = lib.mkOption {
            default = { };
            type = lib.types.attrsOf lib.types.anything;
          };
        };
      }
    ];
    module = import ./hm-module.nix;
  };

  treefmt = optionsDoc {
    # The treefmt module writes into `settings.formatter.pedantix`; declare a
    # freeform `settings` so evalModules can merge its `config` block.
    extraModules = [
      {
        options.settings = lib.mkOption {
          default = { };
          type = lib.types.attrsOf lib.types.anything;
        };
      }
    ];
    module = import ./treefmt-module.nix;
  };
}
