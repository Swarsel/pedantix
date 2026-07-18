# Command line

`pedantix` formats each file **in place**. With no files — or a single `-` —
it reads from `stdin` and writes the formatted result to `stdout`.

## Usage

{{#include gen/cli-options.md}}

## Config discovery

When you don't pass `--config` or `--config-toml`, `pedantix` searches for a`pedantix.toml` or `.pedantix.toml` in the same directory and searching up, stopping at the repo root (or immediately, if not in one). Otherwise it falls back to `$XDG_CONFIG_HOME/pedantix/pedantix.toml`.

In stdin mode, `--stdin-filepath` tells `pedantix` which directory to start that search from — this is how editor integrations get the same config a plain file run would.

## `formatter-command` security

A config that specifies a `formatter-command` (an arbitrary `stdin → stdout` program) is only allowed to run when passed `--config`, `--config-toml`, `--set` via the CLI, or through the global XDG config. An auto-discovered `pedantix.toml` will not run an arbitrary command unless passed  `--allow-formatter-command`.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success — files were formatted, or nothing needed reformatting when using `--check`. |
| `1` | With `--check`: at least one file would be reformatted. |
| `2` | Bad configuration, unreadable file, parse failure, etc. |

## Examples

```console
# Format two files in place
$ pedantix flake.nix hosts/foo.nix

# Check if any file is unformatted
$ pedantix --check $(git ls-files '*.nix')

# Additionally sort let bindings for one run
$ pedantix --set lets.sort=true file.nix

# Explicitly specify base formatter for one run
$ pedantix --formatter alejandra file.nix
```
