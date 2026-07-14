# pedantix

`pedantix` is the pedantic Nix formatter.

It was bothering me that in my NixOS configuration, I was never consistent in ordering my module arguments, some being `{ lib, config, pkgs, ... }` and some others being  `{ config, lib, pkgs, ... }`, you get the idea. Also I had grown tired of [statix](https://github.com/oppiliappan/statix)' `repeated_keys` error. Lastly, due to the nature of the nix language, I do not think there is much value in custom ordering of attributes except for in let bindings (I guess you could make a case for `rec` as well).

`pedantix` does this for you while staying compliant with your formatter of choice (tested with [nixfmt](https://github.com/NixOS/nixfmt), [alejandra](https://github.com/kamadorueda/alejandra), and [nixpkgs-fmt](https://github.com/nix-community/nixpkgs-fmt)). I guess you can consider it a cherry on top :)

All features are optional; these are on by default:

- ordering of function arguments
- ordering of attributes
  - comment blocks stay attached, however they must be exactly in the line prior to the one to be moved

These features are off by default:

- ordering of inherits (a case could be made that this should be on by default)
- ordering of let bindings
- ordering of lists
- merging of repeated keys (this does not fix broken evaluations)
- enforcing of blank lines in between attribute sets
- ordering overrides for custom attribute paths

## Try it now

```console
$ nix run github:Swarsel/pedantix -- file.nix
```

## Configuration

`pedantix` looks for either a `pedantix.toml` or `.pedantix.toml` next to the formatted file or upwards in the same repo. Otherwise it looks for global config in `$XDG_CONFIG_HOME/pedantix/pedantix.toml`.

See [the example pedantix.toml](https://github.com/Swarsel/pedantix/blob/main/pedantix.toml) for all available options.

Lastly, options can also be passed directly, e.g. `pedantix --set lets.sort=true file.nix`.

### Nix

First, add `pedantix` as an input:

```nix
{
  inputs.pedantix.url = "github:Swarsel/pedantix";
}
```

#### Treefmt

To add `pedantix` to [treefmt-nix](https://github.com/numtide/treefmt-nix) when you are using [flake-parts](https://github.com/hercules-ci/flake-parts):

```nix
{
  imports = [
    inputs.treefmt-nix.flakeModule
    inputs.pedantix.flakeModules.default
  ];
  perSystem = _: {
    treefmt.programs.pedantix = {
      enable = true;
      settings.preset = "nixos-module"; # optional; same keys as pedantix.toml
    };
  };
}
```

You can also use it without `flake-parts`:

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

#### Pre-commit checks

You can use `pedantix` with [git-hooks.nix](https://github.com/cachix/git-hooks.nix).

If you use `treefmt`, you just need to enable the `treefmt` hook:

```nix
{
  imports = [
    inputs.treefmt-nix.flakeModule
    inputs.pedantix.flakeModules.default
    inputs.git-hooks-nix.flakeModule
  ];
  perSystem = _: {
    treefmt.programs.pedantix.enable = true;
    pre-commit.settings.hooks.treefmt.enable = true;
  };
}
```

Without `treefmt`, just enable the `pedantix` hook using the `git-hooks` `flakeModule`:

```nix
{
  imports = [
    inputs.git-hooks-nix.flakeModule
    inputs.pedantix.flakeModules.git-hooks
  ];
  perSystem = _: {
    pre-commit.settings.hooks.pedantix.enable = true;
  };
}
```

Without `flake-parts` you can do something like:

```nix
{
  outputs = { self, nixpkgs, git-hooks-nix, pedantix, ... }: {
    checks.x86_64-linux.pre-commit = git-hooks-nix.lib.x86_64-linux.run {
      src = ./.;
      imports = [ pedantix.gitHooksModules.default ];
      hooks.pedantix.enable = true;
    };

    devShells.x86_64-linux.default = nixpkgs.legacyPackages.x86_64-linux.mkShell {
      shellHook = self.checks.x86_64-linux.pre-commit.shellHook;
    };
  };
}
```

#### Home-manager

A [home-manager](https://github.com/nix-community/home-manager) module is available:

```nix
{
  imports = [ inputs.pedantix.homeModules.default ];
  programs.pedantix = {
    enable = true;
    # `settings` uses the same structure as pedantix.toml
    settings = {
      preset = "nixos-module";
    };
  };
}
```

### Emacs

`pedantix` can also be used from [Emacs](https://www.gnu.org/software/emacs). It provides two main, self-explanatory functions:

- `M-x pedantix-format-buffer`
- `M-x pedantix-format-region`

To install it using nix, you can add `overlays.emacs` to your overlays, which adds `pedantix` to every `emacs` package set (so this works with e.g. [emacs-overlay](https://github.com/nix-community/emacs-overlay)). Afterwards:

```nix
programs.emacs.extraPackages = epkgs: [ epkgs.pedantix ];
```

Alternatively, without the overlay:

```nix
programs.emacs.extraPackages = epkgs: [ (inputs.pedantix.lib.emacsPackage epkgs) ];
```

If you install `pedantix.el` in some way, the `pedantix` binary must be on PATH (or set `pedantix-program`).

Then, in `emacs`:

```elisp
(require 'pedantix)
(add-hook 'nix-mode-hook #'pedantix-format-on-save-mode)
```

Or with `use-package`:

```elisp
(use-package pedantix
  :hook ((nix-mode nix-ts-mode) . pedantix-format-on-save-mode))
```
