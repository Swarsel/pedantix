# (Per-file) Overrides

Sometimes you might want to supply a custom ordering for certain constructs.

## Overrides

The `[[overrides]]` array applies targeted rules to matching attribute paths.

Each override has a `path` glob and may set the partial rules (`sort` / `first` / `last` / `[attrs]`-only keys) of any construct: `attrs`, `args`, `lets`, `inherits`, `lists`.

### Precedence

Overrides layer on top of the global construct rules (and any preset). When multiple overrides match a path, later entries in the array win for the keys they set.

### Path globs

`path` is a glob over **dot-separated attribute paths**:

- `*` matches exactly one path component;
- `**` matches any number of components, including zero.

### Examples

Leave a git alias table in its original order:

```toml
[[overrides]]
path = "**.programs.git.settings.alias"
attrs.sort = false
```

Put `description` and `wantedBy` first inside each systemd service:

```toml
[[overrides]]
path = "**.systemd.services.*"
attrs.first = ["description", "wantedBy"]
```

Sort the elements of `environment.systemPackages`:

```toml
[[overrides]]
path = "**.environment.systemPackages"
lists.sort = true
```

Restore one blank line between the inputs of a `flake.nix`:

```toml
[[overrides]]
path = "inputs"
attrs.blank-lines = 1
```

## Per-file configuration

The `[[files]]` array applies targeted rules to matching file paths.

Each file configuration can take a full configuration using all other options (except another `files`).

### Precedence

The configuration will first source the entries' `preset` (overriding the top-level preset, and if not given using that one), then layer on top the top-level keys, and finally override them with the entries' keys.

As for `[[overrides]]`, the entries' overrides are applied after the top-level ones (so the most specific entry wins).

### Path globs

`pattern` is a glob over `/`-separated file paths:

- `*` matches exactly one path component;
- `**` matches any number of components, including zero.

By default, every pattern matches anywhere beneath the directory holding the config file. To override this, prefix the pattern with `./` following the to-be-matched pattern.

### Example

This will make `hello.pkg.nix` use the `nixpkgs-package` preset while everything else keeps using `nixos-module`.

```toml
preset = "nixos-module"

[[files]]
pattern = "*.pkg.nix"
preset = "nixpkgs-package"
```
