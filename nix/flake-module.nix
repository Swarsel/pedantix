{ pedantixTreefmtModule }:
{ flake-parts-lib, ... }:
{
  options.perSystem = flake-parts-lib.mkPerSystemOption {
    config.treefmt = {
      imports = [ pedantixTreefmtModule ];
    };
  };
}
