# Presets

A preset provides a set of base values that your explicit settings layer on top of. Select one with the top-level `preset` key:

```toml
preset = "nixos-module"

# anything below overrides the preset
[lets]
sort = true
```


> **Note:**
>
> When you set `[[overrides]]` yourself, note that a matching override list *replaces* the discovered list, while preset-provided overrides still combine!

The following presets are currently in `pedantix`:

{{#include gen/presets.md}}
