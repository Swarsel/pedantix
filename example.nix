{
  flake.modules.homeManager.git =
    {
      lib,
      config,
      confLib,
      globals ? {
        user.name = "duck";
      },
            nixosConfig ? null,
      minimal,

      ...
    }:
    let
      gitUser = globals.user.name;
      inherit (confLib.getConfig.repo.secrets.common.mail) bababear address1;
      inherit (confLib.getConfig.repo.secrets.common) fullName;


    in
    {
      config = {
        programs.difftastic.enable = lib.mkIf (!minimal) true;
        programs.git = {
          enable = true;
        }
        // lib.optionalAttrs (!minimal) {
          includes = [
            {
              contents = {
                commit = {
                  template = "~/.gitmessage";
                };
                github = {
                  user = gitUser;
                };
              };
            }
          ];
          lfs.enable = true;
          settings = {
            alias = {
              a = "add";
              b = "branch";
              c = "commit";
              cl = "clone";
              co = "checkout";
              i = "init";
              m = "merge";
              p = "pull";
              pp = "push";
              r = "restore";
              s = "status";
            };
            user = {
              email = lib.mkIf ((nixosConfig != null) && !config.swarselsystems.isPublic) (
                lib.mkDefault address1
              );
              name = lib.mkIf ((nixosConfig != null) && !config.swarselsystems.isPublic) fullName;
            };
          };
          signing = {
            format = "openpgp";
            key = "0x76FD3810215AE097";
            signByDefault = true;
          };
        };
        swarselsystems.enabledHomeModules = [ "git" ];
      };
    };
}
