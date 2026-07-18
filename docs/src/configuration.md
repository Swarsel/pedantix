# Configuration

`pedantix` is configured with a TOML file (see [config discovery](cli.md#config-discovery)).

Individual values can also be passed on the command line with `--set`, e.g. `pedantix --set lets.sort=true file.nix`, or the whole document inline with `--config-toml`.

## `preset`

This applies a base layer of configuration which you can layer your own config atop of - see [Presets](presets.md).

```toml
preset = "nixos-module"
```


## Reference

{{#include gen/config-options.md}}

## Notes

### The five construct tables share one shape

`[args]`, `[attrs]`, `[lets]`, `[inherits]`, and `[lists]` all take the same **sort rules** keys. Names not listed in `first` / `last` are sorted alphabetically between them. The `merge` and `blank-lines*` keys are only meaningful under `[attrs]`.

```toml
[args]
first = ["self", "lib", "config", "pkgs"]
last  = ["<defaulted>", "..."]

[attrs]
first = ["imports", "enable", "package"]

[lets]
sort = true
```

`[lets]`, `[inherits]`, and `[lists]` are **off by default**. List sorting in particular is best enabled per-path via an [override](overrides.md) rather than globally, since list order is often significant.

### `merge` collapses shared attrpaths

```toml
[attrs]
merge = true
```

turns `a.b = 1; a.c = 2;` into `a = { b = 1; c = 2; };`. This does not fix broken evaluation by e.g. dynamically derived attribute set names.

### Top-level blank lines vs. `[attrs] blank-lines`

The `top-level-blank-lines*` keys and the `[attrs]` `blank-lines*` keys look similar but do slightly different things:

> `top-level-blank-lines` does **not** descend into function-call sets, and  treats a `flake.nix` `inputs`/`outputs` body as its own top-level set.
>
> `[attrs] blank-lines` applies to *every* attrset the `[attrs]` rules match, at any nesting depth (including sets inside function calls), and can be targeted per path via [`[[overrides]]`](overrides.md).

A reordered set by default drops all blank lines, so set one of these to restore spacing.

### `[inherits]` vs. `inherit-placement`

`[inherits]` sorts the *names inside* an `inherit`. The top-level `inherit-placement` key is separate: it controls where the whole `inherit` *statement* sits relative to other bindings.

## Per-path overrides

The `[[overrides]]` array lets you change any of the rules above for specific attribute paths — see [Overrides](overrides.md).
