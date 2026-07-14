{
  description = "pedantix — the pedantic Nix formatter";

  inputs = {
    flake-parts = {
      inputs.nixpkgs-lib.follows = "nixpkgs";
      url = "github:hercules-ci/flake-parts";
    };
    git-hooks-nix = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:cachix/git-hooks.nix";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    treefmt-nix = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:numtide/treefmt-nix";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      { flake-parts-lib, ... }:
      let
        emacsPackage = import ./nix/emacs-package.nix {
          localFlake = inputs.self;
        };
        flakeModule = flake-parts-lib.importApply ./nix/flake-module.nix {
          pedantixTreefmtModule = treefmtModule;
        };
        gitHooksFlakeModule = flake-parts-lib.importApply ./nix/git-hooks-flake-module.nix {
          pedantixGitHooksModule = gitHooksModule;
        };
        gitHooksModule = flake-parts-lib.importApply ./nix/git-hooks-module.nix {
          localFlake = inputs.self;
        };
        hmModule = flake-parts-lib.importApply ./nix/hm-module.nix {
          localFlake = inputs.self;
        };
        treefmtModule = flake-parts-lib.importApply ./nix/treefmt-module.nix {
          localFlake = inputs.self;
        };
      in
      {
        imports = [
          inputs.treefmt-nix.flakeModule
          inputs.git-hooks-nix.flakeModule
          flakeModule
          gitHooksFlakeModule
        ];
        flake = {
          flakeModules = {
            default = flakeModule;
            git-hooks = gitHooksFlakeModule;
            pedantix = flakeModule;
          };
          gitHooksModules = {
            default = gitHooksModule;
            pedantix = gitHooksModule;
          };
          homeManagerModules = {
            default = hmModule;
            pedantix = hmModule;
          };
          homeModules = {
            default = hmModule;
            pedantix = hmModule;
          };
          lib = {
            inherit emacsPackage;
          };
          overlays = {
            default = final: _prev: {
              pedantix = final.callPackage ./nix/package.nix { };
              pedantix-wrapped = final.pedantix.wrapped;
            };
            emacs = _final: prev: {
              emacsPackagesFor =
                emacs:
                (prev.emacsPackagesFor emacs).overrideScope (
                  efinal: _eprev: {
                    pedantix = emacsPackage efinal;
                  }
                );
            };
          };
          treefmtModules = {
            default = treefmtModule;
            pedantix = treefmtModule;
          };
        };
        perSystem =
          { config, pkgs, ... }:
          let
            pedantix = pkgs.callPackage ./nix/package.nix { };
            runApp = {
              meta.description = "The pedantic Nix formatter";
              program = config.packages.pedantix-wrapped;
            };
          in
          {
            apps = {
              default = runApp;
              pedantix = runApp;
            };
            checks = {
              example-content-preserved =
                pkgs.runCommand "pedantix-example-content-preserved"
                  {
                    nativeBuildInputs = [
                      config.packages.pedantix-wrapped
                      pkgs.nix
                    ];
                  }
                  ''
                    export NIX_STORE_DIR=$TMPDIR/store NIX_STATE_DIR=$TMPDIR/state NIX_LOG_DIR=$TMPDIR/log
                    install -m644 ${./example.nix} example.nix
                    install -m644 ${./pedantix.toml} pedantix.toml
                    nix-instantiate --parse example.nix > before.parse
                    pedantix example.nix
                    pedantix --check example.nix # idempotent
                    nix-instantiate --parse example.nix > after.parse
                    diff before.parse after.parse
                    touch $out
                  '';
            };
            devShells.default = pkgs.mkShell {
              inputsFrom = [ config.treefmt.build.devShell ];
              packages = with pkgs; [
                alejandra
                cargo
                clippy
                nixfmt
                nixpkgs-fmt
                rust-analyzer
                rustc
                rustfmt
              ];
              shellHook = config.pre-commit.installationScript;
            };
            packages = {
              inherit pedantix;
              default = pedantix;
              pedantix-wrapped = pedantix.wrapped;
            };
            pre-commit.settings.hooks = {
              deadnix = {
                enable = true;
                excludes = [ "example.nix" ];
              };
              # this is intended
              pedantix = {
                enable = true;
                excludes = [ "example.nix" ];
              };
              statix.enable = true;
              treefmt.enable = true;
            };
            treefmt = {
              programs = {
                pedantix = {
                  enable = true;
                  package = config.packages.pedantix-wrapped;
                  excludes = [ "example.nix" ];
                };
                rustfmt.enable = true;
                taplo.enable = true;
              };
              projectRootFile = "flake.nix";
            };
          };
        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "x86_64-darwin"
          "aarch64-darwin"
        ];
      }
    );
}
