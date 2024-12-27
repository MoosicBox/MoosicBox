let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-24.11";
  pkgs = import nixpkgs {
    config = { };
    overlays = [ ];
  };
in

pkgs.mkShellNoCC {
  packages = with pkgs; [
    pkg-config
    gnumake
    gcc
    libiconv
    autoconf
    automake
    libtool
    cmake
    openssl
    alsa-lib
    alsa-lib.dev
  ];

  shellHook = ''
    export LD_LIBRARY_PATH=${pkgs.alsa-lib}/lib:$LD_LIBRARY_PATH
  '';

}
