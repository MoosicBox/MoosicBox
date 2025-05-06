let
  pkgs = import <nixpkgs> {
    config = { };
    overlays = [ ];
  };
in

pkgs.mkShellNoCC {
  packages = with pkgs; [
    pkg-config
    gnumake
    clang
    libiconv
    autoconf
    automake
    libtool
    cmake
    openssl
  ];

  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"
  '';

}
