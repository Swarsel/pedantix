# pedantix

`pedantix` is the pedantic Nix formatter.

It  runs your base formatter of choice ([`nixfmt`](https://github.com/NixOS/nixfmt), [`alejandra`](https://github.com/kamadorueda/alejandra), [`nixpkgs-fmt`](https://github.com/nix-community/nixpkgs-fmt), or an arbitrary command of your choosing) and then applies reordering on top. Every feature is optional.

On by default:

- ordering of function arguments
- ordering of attributes (comment blocks above the moved line stay attached)

Off by default:

- ordering of inherits (a case could be made that this should be on by default)
- ordering of let bindings
- ordering of lists
- merging of repeated keys (this does not fix broken evaluations)
- enforcing of blank lines in between attribute sets
- ordering overrides for custom attribute paths

<br>

**Continue to [Getting Started](getting-started.md).**
