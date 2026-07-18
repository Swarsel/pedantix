# treefmt

`pedantix` integrates with [treefmt-nix](https://github.com/numtide/treefmt-nix) as a formatter program.

## With flake-parts

Import the flake module and enable the program under `perSystem`:

```nix
{
  imports = [
    inputs.treefmt-nix.flakeModule
    inputs.pedantix.flakeModules.default
  ];
  perSystem = _: {
    treefmt.programs.pedantix = {
      enable = true;
    };
  };
}
```

## Without flake-parts

Use the standalone treefmt module with `treefmt-nix`'s `evalModule`:

```nix
let
  treefmtEval = inputs.treefmt-nix.lib.evalModule pkgs {
    imports = [ inputs.pedantix.treefmtModules.default ];
    projectRootFile = "flake.nix";
    programs.pedantix.enable = true;
  };
in
{
  formatter.${system} = treefmtEval.config.build.wrapper;
  checks.${system}.formatting = treefmtEval.config.build.check self;
}
```

## Options

`settings` is translated into `--set key=value` flags, so each key is merged **on top of** any `pedantix.toml` discovered in the project. This yields one quirk: setting `overrides` in `settings` *replaces* a discovered `overrides` list, whereas preset-provided overrides still combine. If you want to keep a project's `pedantix.toml` authoritative, prefer `configFile` (or rely on discovery) over restating everything in `settings`.

{{#include gen/treefmt-options.md}}

## Full example


```nix
treefmt.programs.pedantix = {
  enable = true;
  settings = {
    preset = "nixos-module";
    formatter = "alejandra";
    lets.sort = true;
    overrides = [
      {
        path = "**.programs.git.settings.alias";
        attrs.sort = false;
      }
    ];
  };
};
```
