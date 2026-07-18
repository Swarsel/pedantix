# Getting started

## Try it now

The following gives you a preview of your file, formatted using `pedantix`. You can run this from any machine that has `nix` installed:

```console
$ nix run github:swarsel/pedantix < your-file.nix
```


Normally, you will probably want to format a file in place:

```console
$ pedantix file.nix
```

With no file arguments (or a single `-`), `pedantix` reads from stdin and writes the result to stdout.

## Installing

Add `pedantix` as a flake input:

```nix
{
  inputs.pedantix.url = "github:swarsel/pedantix";
}
```

From there you can:

- use it with [treefmt](nix-treefmt.md);
- run it as a [pre-commit hook](nix-git-hooks.md);
- manage the global config with [home-manager](nix-home-manager.md);
- use it from [Emacs](emacs.md).

## A first configuration

`pedantix` works with zero configuration, but the whole idea is that you tune a `pedantix.toml` to your own pedantic tastes; a minimal one that turns on `let` sorting and sets a custom argument order could look like this:

```toml
formatter = "nixfmt"

[args]
first = ["self", "lib", "config", "pkgs"]
last = ["<defaulted>", "..."]

[lets]
sort = true
```

The full set of options is documented in [Configuration](configuration.md).
