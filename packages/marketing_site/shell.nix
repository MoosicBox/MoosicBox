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
    clang
    stdenv
    libiconv
    autoconf
    automake
    libtool
    cmake
    alsa-lib
    alsa-lib.dev
    alsa-utils
    udev.dev
    wayland
    libxkbcommon
    webkitgtk_4_1
    libsoup_3
    gtk3
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    xorg.libX11
    xorg.libxcb
    xorg.xcbutil
    xorg.xcbutilimage
    xorg.xcbutilkeysyms
    xorg.xcbutilwm # contains xcb-ewmh among others
    vulkan-loader
  ];

  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.wayland ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.libxkbcommon ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.amdvlk ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.webkitgtk_4_1 ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libX11 ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libxcb ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutil ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilimage ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilkeysyms ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilwm ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.clang ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib ]}"
    export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.wayland}/lib"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.libxkbcommon}/lib"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.webkitgtk_4_1}/lib"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.vulkan-loader}/lib"
  '';

}
