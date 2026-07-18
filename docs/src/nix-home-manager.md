# Home Manager

The Home Manager module installs `pedantix` and, optionally, writes a global `pedantix.toml` to `$XDG_CONFIG_HOME/pedantix/pedantix.toml`.

## Options

{{#include gen/hm-options.md}}

## Full Example

```nix
{
  imports = [ inputs.pedantix.homeModules.default ];

  programs.pedantix = {
    enable = true;
    settings = {
      preset = "nixos-module";
      lets.sort = true;
    };
  };
}
```
