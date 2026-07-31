# Nix integration

`pedantix` provides a flake. To add it as an input:

```nix
{
  inputs.pedantix.url = "github:swarsel/pedantix";
}
```

## Packages

Per system, the flake exposes:

| Attribute | What it is |
|-----------|------------|
| `packages.${system}.pedantix` | The bare binary. You must provide the base formatter (`nixfmt`/`alejandra`/`nixpkgs-fmt`) on `PATH` yourself. |
| `packages.${system}.pedantix-wrapped` | A wrapper that bundles `nixfmt`, `alejandra`, and `nixpkgs-fmt` on `PATH`. This is what the flake app and all the modules use by default. |

To install it system-wide:

```nix
environment.systemPackages = [ pedantix.packages.${system}.pedantix-wrapped ];
```

Or, to install via [home-manager](https://github.com/nix-community/home-manager) for a user:

```nix
home.packages = [ pedantix.packages.${system}.pedantix-wrapped ];
```

> **Note:**
>
> When using a drop-in replacement formatter (e.g. [nixfmt-rs](https://github.com/Mic92/nixfmt-rs)), you should either use the unwrapped package with your replacement package installed, or use your own wrapper. Then, set that formatter's name in your config as normal.

## App

You can run `pedantix` directly from any system that has `nix` installed:

```console
$ nix run github:swarsel/pedantix -- file.nix
```

## Overlays

The following overlays are provided by the flake:

| Overlay | Effect |
|---------|--------|
| `overlays.default` | Adds `pkgs.pedantix` and `pkgs.pedantix-wrapped`. |
| `overlays.emacs` | Adds `pedantix` to every `emacsPackagesFor` set (works with e.g. [emacs-overlay](https://github.com/nix-community/emacs-overlay)). See [Emacs integration](emacs.md). |

## Modules

`pedantix` ships modules for some common Nix tooling. See the following table to learn which module you have to import for what:.

| Purpose | flake-parts module | Standalone module | Page |
|---------|--------------------|-------------------|------|
| [treefmt](https://github.com/numtide/treefmt-nix)  | `flakeModules.default` / `flakeModules.pedantix` | `treefmtModules.default` | [treefmt](nix-treefmt.md) |
| [git-hooks / pre-commit](https://github.com/cachix/git-hooks.nix) | `flakeModules.git-hooks` | `gitHooksModules.default` | [Pre-commit](nix-git-hooks.md) |
| [home-manager](https://github.com/nix-community/home-manager) | — | `homeModules.default` (`homeManagerModules.default`) | [Home Manager](nix-home-manager.md) |

There is also `lib.emacsPackage`, a function `epkgs -> package` for installing
the Emacs package without the overlay — see [Emacs](emacs.md).
