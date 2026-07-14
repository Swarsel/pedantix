{ pedantixGitHooksModule }:
{ flake-parts-lib, ... }:
{
  options.perSystem = flake-parts-lib.mkPerSystemOption {
    config.pre-commit.settings = {
      imports = [ pedantixGitHooksModule ];
    };
  };
}
