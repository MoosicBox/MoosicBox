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
    llvmPackages.libclang
    glibc.dev
    sqlite
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
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.alsa-lib ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.llvmPackages.libclang ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.sqlite ]}"
    export LIBCLANG_PATH="${pkgs.lib.makeLibraryPath [ pkgs.llvmPackages.libclang ]}"
    export CPLUS_INCLUDE_PATH="${pkgs.glibc.dev}/include";

    export BINDGEN_EXTRA_CLANG_ARGS="$(< ${pkgs.stdenv.cc}/nix-support/libc-crt1-cflags) \
          $(< ${pkgs.stdenv.cc}/nix-support/libc-cflags) \
          $(< ${pkgs.stdenv.cc}/nix-support/cc-cflags) \
          $(< ${pkgs.stdenv.cc}/nix-support/libcxx-cxxflags) \
          ${
            pkgs.lib.optionalString pkgs.stdenv.cc.isClang
            "-idirafter ${pkgs.stdenv.cc.cc}/lib/clang/${
              pkgs.lib.getVersion pkgs.stdenv.cc.cc
            }/include"
          } \
          ${
            pkgs.lib.optionalString pkgs.stdenv.cc.isGNU
            "-isystem ${pkgs.stdenv.cc.cc}/include/c++/${
              pkgs.lib.getVersion pkgs.stdenv.cc.cc
            } -isystem ${pkgs.stdenv.cc.cc}/include/c++/${
              pkgs.lib.getVersion pkgs.stdenv.cc.cc
            }/${pkgs.stdenv.hostPlatform.config} -idirafter ${pkgs.stdenv.cc.cc}/lib/gcc/${pkgs.stdenv.hostPlatform.config}/${
              pkgs.lib.getVersion pkgs.stdenv.cc.cc
            }/include"
          } \
        "
    '';
}
