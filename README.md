# pedantix

`pedantix` is the pedantic Nix formatter.

It was bothering me that in my NixOS configuration, I was never consistent in ordering my module arguments, some being `{ lib, config, pkgs, ... }` and some others being  `{ config, lib, pkgs, ... }`, you get the idea. Also I had grown tired of [statix](https://github.com/oppiliappan/statix)' `repeated_keys` error. Lastly, due to the nature of the nix language, I do not think there is much value in custom ordering of attributes except for in let bindings (I guess you could make a case for `rec` as well).

`pedantix` does this for you while staying compliant with your formatter of choice (tested with [nixfmt](https://github.com/NixOS/nixfmt)[(-rs)](https://github.com/Mic92/nixfmt-rs), [alejandra](https://github.com/kamadorueda/alejandra), and [nixpkgs-fmt](https://github.com/nix-community/nixpkgs-fmt)). I guess you can consider it a cherry on top :)

All features are optional; these are on by default:

- ordering of function arguments
- ordering of attributes (comment blocks above the moved line stay attached)

These features are off by default:

- ordering of inherits (a case could be made that this should be on by default)
- ordering of let bindings
- ordering of lists
- merging of repeated keys (this does not fix broken evaluations)
- flattening of attribute sets with a single entry
- (un-)quoting of attribute names
- enforcing of blank lines in between attribute sets
- ordering overrides for custom attribute paths

## Try it now

<sup><sup>(this will not modify your file, but only show you how it would look when formatted with `pedantix`)</sup></sup>

```bash
nix run github:Swarsel/pedantix < your-file.nix
```

## [Click here to see the full Documentation](https://swarsel.github.io/pedantix)
