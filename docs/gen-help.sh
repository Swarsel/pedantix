#!/usr/bin/env bash
# Generate the docs' reference sections from the source of truth:
#   - docs/src/gen/cli-options.md     from the clap Command in src/cli.rs
#   - docs/src/gen/config-options.md  from the JSON schema on config.rs
#   - docs/src/gen/presets.md         from presets/*.toml
#   - docs/src/gen/hm-options.md      from nix/hm-module.nix
#   - docs/src/gen/treefmt-options.md from nix/treefmt-module.nix
#   - docs/src/gen/emacs.md           from emacs/pedantix.el
# so the references never drift from the code.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
out="$here/src/gen"
mkdir -p "$out"
cd "$repo"

have_cargo() { command -v cargo >/dev/null 2>&1; }

# --- Rust generators (CLI, config, presets) --------------------------------
run_example() {
  local example="$1"
  shift
  if have_cargo; then
    cargo run --quiet --example "$example" "$@"
  else
    nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc \
      --command cargo run --quiet --example "$example" "$@"
  fi
}

emit() {
  local target="$1"
  shift
  local tmp
  tmp="$(mktemp)"
  if "$@" >"$tmp"; then
    mv "$tmp" "$out/$target"
    echo "wrote $out/$target"
  else
    rm -f "$tmp"
    echo "failed to generate $target" >&2
    exit 1
  fi
}

emit cli-options.md run_example gen-cli-docs
emit config-options.md run_example gen-config-docs --features docs
emit presets.md run_example gen-presets-docs

# --- Nix module options (treefmt, home-manager) ----------------------------
# Built from the working tree via nix/options-doc.nix so it does not depend on
# the flake being committed.
build_options() {
  local attr="$1"
  nix-build --no-out-link --expr "
    let
      pkgs = import <nixpkgs> { };
      docs = import ./nix/options-doc.nix {
        inherit pkgs;
        localFlake.packages.\${pkgs.stdenv.hostPlatform.system}.pedantix-wrapped = pkgs.hello;
      };
    in docs.${attr}
  "
}

for pair in "hm:hm-options.md" "treefmt:treefmt-options.md"; do
  attr="${pair%%:*}"
  target="${pair##*:}"
  install -m644 "$(build_options "$attr")" "$out/$target"
  echo "wrote $out/$target"
done

# --- Emacs options (defcustoms + commands) ---------------------------------
emit_emacs() {
  if command -v emacs >/dev/null 2>&1; then
    emacs --batch -l docs/gen-emacs-docs.el
  else
    nix shell nixpkgs#emacs-nox --command emacs --batch -l docs/gen-emacs-docs.el
  fi
}
emit emacs.md emit_emacs
