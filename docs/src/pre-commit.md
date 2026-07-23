# Pre-commit framework

`pedantix` works with [git-hooks.nix](https://github.com/cachix/git-hooks.nix) (formerly known as pre-commit-hooks.nix).

## Using Nix flakes

The hooks `entry` defaults to running `pedantix --check` on staged `*.nix` files, so it just fails the commit when something is unformatted, not editing any files.

### Using treefmt hook

If you already format with [treefmt](nix-treefmt.md), you only need to enable the `treefmt` hook — it runs `pedantix` as part of the treefmt pass:

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

### Without treefmt

Without treefmt, enable the `pedantix` hook through the git-hooks flake module:

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

### Without flake-parts

Use the standalone module directly:

```nix
{
  outputs = { self, nixpkgs, git-hooks-nix, pedantix, ... }: {
    checks.x86_64-linux.pre-commit = git-hooks-nix.lib.x86_64-linux.run {
      src = ./.;
      imports = [ pedantix.gitHooksModules.default ];
      hooks.pedantix.enable = true;
    };

    devShells.x86_64-linux.default =
      nixpkgs.legacyPackages.x86_64-linux.mkShell {
        shellHook = self.checks.x86_64-linux.pre-commit.shellHook;
      };
  };
}
```

## Without Nix flakes

`pedantix` also ships a plain [`.pre-commit-hooks.yaml`](https://github.com/swarsel/pedantix/blob/main/.pre-commit-hooks.yaml) with two hooks:

| Hook           | Runs pedantix via       | Needs          |
| -------------- | ----------------------- | -------------- |
| `pedantix`     | a source build (`rust`) | Rust toolchain |
| `pedantix-nix` | `nix run`               | Nix            |

```yaml
repos:
- repo: https://github.com/swarsel/pedantix
  rev: v1.1.0
  hooks:
  - id: pedantix
```

Without a formatter on `PATH` the hook will fail; set `formatter = "off"` in `pedantix.toml` to only reorder.
