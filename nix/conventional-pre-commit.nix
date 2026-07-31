{
  lib,
  fetchFromGitHub,
  python3Packages,
}:
python3Packages.buildPythonApplication rec {
  build-system = with python3Packages; [
    setuptools
    setuptools-scm
  ];
  env.SETUPTOOLS_SCM_PRETEND_VERSION = version;
  meta = {
    description = "Pre-commit hook that checks commit messages for Conventional Commits formatting";
    license = lib.licenses.asl20;
    mainProgram = "conventional-pre-commit";
  };
  pname = "conventional-pre-commit";
  pyproject = true;
  src = fetchFromGitHub {
    hash = "sha256-8wpsdrTv2N2FFMZzRzJ3ufFtTehoZTaiHvXxNbV6vIQ=";
    owner = "compilerla";
    repo = "conventional-pre-commit";
    tag = "v${version}";
  };
  version = "4.4.0";
}
