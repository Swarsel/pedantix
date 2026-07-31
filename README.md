# pedantix

`pedantix` is the pedantic Nix formatter.

It was bothering me that in my NixOS configuration, I was never consistent in ordering my module arguments, some being `{ lib, config, pkgs, ... }` and some others being  `{ config, lib, pkgs, ... }`, you get the idea. Also I had grown tired of [statix](https://github.com/oppiliappan/statix)' `repeated_keys` error. Lastly, due to the nature of the nix language, I do not think there is much value in custom ordering of attributes except for in let bindings (I guess you could make a case for `rec` as well).

`pedantix` does this for you while staying compliant with your formatter of choice (tested with [nixfmt](https://github.com/NixOS/nixfmt)[(-rs)](https://github.com/Mic92/nixfmt-rs), [alejandra](https://github.com/kamadorueda/alejandra), and [nixpkgs-fmt](https://github.com/nix-community/nixpkgs-fmt)). I guess you can consider it a cherry on top :)

Here are some of its features (everything is optional):

- ordering of function arguments
- ordering of attributes
- ordering of inherits
- ordering of let bindings
- ordering of lists (use with caution!)
- merging of repeated keys (this does not fix broken evaluations)
- flattening of attribute sets with a single entry
- (un-)quoting of (valid) identifiers
- enforcing of blank lines in between attribute sets
- overrides for arbitrary attribute paths

## Try it now

<sup><sup>(this will not modify your file, but only show you how it would look when formatted with `pedantix`)</sup></sup>

```bash
nix run github:Swarsel/pedantix < your-file.nix
```

## [Click here to see the full Documentation](https://swarsel.github.io/pedantix)
