# Overrides

Sometimes you might want to supply a custom ordering for a specific attribute path.  The `[[overrides]]` array applies targeted rules to matching paths.

Each override has a `path` glob and may set the partial rules (`sort` / `first` / `last` / `[attrs]`-only keys) of any construct: `attrs`, `args`, `lets`, `inherits`, `lists`.

## Precedence

Overrides layer on top of the global construct rules (and any preset). When multiple overrides match a path, later entries in the array win for the keys they set.

## Path globs

`path` is a glob over **dot-separated attribute paths**:

- `*` matches exactly one path component;
- `**` matches any number of components, including zero.

## Examples

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
