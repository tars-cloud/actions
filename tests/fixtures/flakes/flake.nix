{
  description = "Small devenv-integrated fixture for shared action tests";

  inputs = {
    nixpkgs = {
      url = "github:cachix/devenv-nixpkgs/c2f38fe7f9e04d9aadd354d380f2bd40531d9737";
    };
    devenv = {
      url = "github:cachix/devenv/1c57b5dea0d400af97053fdd1a536fca17378f73";
      inputs = {
        nixpkgs = {
          follows = "nixpkgs";
        };
      };
    };
  };

  outputs =
    inputs@{ nixpkgs, devenv, ... }:
    {
      packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (system: {
        devenv = (import nixpkgs { inherit system; }).devenv;
      });
      devShells = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          shell =
            name: withTrivy:
            devenv.lib.mkShell {
              inherit inputs pkgs;
              modules = [
                {
                  devenv = {
                    root = builtins.getEnv "PWD";
                  };
                  cachix = {
                    enable = false;
                  };
                  packages = [ pkgs.bash ] ++ nixpkgs.lib.optionals withTrivy [ pkgs.trivy ];
                  env = {
                    FIXTURE_SHELL = name;
                    FIXTURE_SYSTEM = system;
                  };
                  enterTest = ''
                    test "$FIXTURE_SHELL" = "${name}"
                  '';
                }
              ];
            };
        in
        {
          default = shell "default" true;
          named = shell "named" true;
          missing-trivy = shell "missing-trivy" false;
        }
      );
    };
}
