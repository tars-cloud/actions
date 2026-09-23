{
  pkgs,
  rustPlatform,
}:

(pkgs.convco.override { inherit rustPlatform; }).overrideAttrs (
  final: previous: {
    version = "0.7.2";
    src = pkgs.fetchFromGitHub {
      owner = "convco";
      repo = "convco";
      rev = "021e3effa494b351bd5a6f4ed07b1cd9fd406988";
      hash = "sha256-vzxIZdXmWASnafaeXt/QBgg9WXyMFwPclFeD+4oF4V0=";
    };
    cargoDeps = rustPlatform.fetchCargoVendor {
      inherit (final) src;
      hash = "sha256-f/jfriJeBgR3OkWj5S/NkTc/bhGjqU/yQHUfddUo2yQ=";
    };
    nativeCheckInputs = (previous.nativeCheckInputs or [ ]) ++ [ pkgs.git ];
  }
)
