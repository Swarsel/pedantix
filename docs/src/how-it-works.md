# How it works

`pedantix` parses Nix with [tree-sitter](https://tree-sitter.github.io/) and rewrites the syntax tree. A run works roughly like this:

1. Perform an optional base formatting
2. Sort expressions according to your config
3. Perform an optional merging of attribute paths
4. Optionally enforce spacing between attribute paths/sets
1. Perform an optional base formatting

## Safety

- `pedantix` preserves semantics, which *should* guarantee that it will not break your config
- repeated runs are idempotent (the only exception would be if you enforce a higher line spacing than allowed by your base formatter and then turning the after-pass off)
- It stays compliant with your base formatter (unless you do what I described above)

## Comments

When attributes are reordered, comments move with the binding they belong to, i.e. the binding below the comments. So in general a comment on its own line will travel along with the correct line of code.

> **Corollary:**
>
> - If you use comments to demarcate "sections" of sorts, that will obviously
>   break when running `pedantix`.
> - "Free-floating" comments for e.g. "archived" code will as a result move
>   somewhat randomly.

## The base formatter

There are a number of good Nix formatters out there and I considered it a foolish idea to introduce a new one into the ecosystem that nobody would want to adapt; iInstead, you can bring your own formatter and still use `pedantix` in any case.

If the default set of base formatters (`nixfmt`, `alejandra`, `nixpkgs-fmt`) is not to your liking, you can supply any command of your choosing that reads from `stdin` and emits to `stdout`. For safety reasons, the latter is only accepted when passed directly on the CLI or when passing `--allow-formatter-command`. See [Configuration](configuration.md#the-base-formatter) for details.
