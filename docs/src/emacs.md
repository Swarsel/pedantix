# Emacs integration

`pedantix` ships an Emacs package, `pedantix.el`, that formats Nix buffers by calling `pedantix`. It provides two main commands plus a format-on-save minor mode.

The package requires Emacs 27.1 or newer.

## Commands and customization

{{#include gen/emacs.md}}

> **Why the region must be self-contained.** `pedantix` and the base formatters parse *complete* expressions. A bare run of bindings like `b = 1; a = 2;` is not a valid expression on its own and cannot be formatted — select the enclosing `{ … }` instead. If formatting a region fails, the error buffer hints at exactly this.

## Usage

```elisp
(require 'pedantix)

(add-hook 'nix-mode-hook    #'pedantix-format-on-save-mode)
(add-hook 'nix-ts-mode-hook #'pedantix-format-on-save-mode)
```

Or with `use-package`:

```elisp
(use-package pedantix
  :hook ((nix-mode nix-ts-mode) . pedantix-format-on-save-mode))
```

## Installing with Nix

The cleanest way is the `overlays.emacs` overlay, which adds `pedantix` to every `emacs` package set (this works with e.g. [emacs-overlay](https://github.com/nix-community/emacs-overlay)).

Add the overlay, then:

```nix
programs.emacs.extraPackages = epkgs: [ epkgs.pedantix ];
```

Without the overlay, use `lib.emacsPackage`:

```nix
programs.emacs.extraPackages = epkgs: [ (inputs.pedantix.lib.emacsPackage epkgs) ];
```

Both install `pedantix.el` with `pedantix-program` already patched to the wrapped binary's store path, so the executable and the base formatters are on hand without extra `PATH` setup.

## Installing manually

If you install `pedantix.el` by any other means, make sure the `pedantix` binary is on `PATH` (or set `pedantix-program` to its path):
