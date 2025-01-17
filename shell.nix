let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-24.11";
  pkgs = import nixpkgs {
    config = { };
    overlays = [ ];
  };
in

pkgs.mkShellNoCC {
  buildInputs = with pkgs; [
    fontconfig
  ];
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
    xorg.libXcursor
    xorg.libXfixes
    xorg.libXinerama
    xorg.libxcb
    xorg.xcbutil
    xorg.xcbutilimage
    xorg.xcbutilkeysyms
    xorg.xcbutilwm # contains xcb-ewmh among others
    vulkan-loader
    pango
    cairo
    gdk-pixbuf
    glib

    at-spi2-atk # for one example (file dialog)
    gdk-pixbuf # for one example (file dialog)
    gtkd
    gtk3.dev
    gtk3-x11
    gtk3-x11.dev
    gsettings-desktop-schemas
    glib
  ];

  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.wayland ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.libxkbcommon ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.amdvlk ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.webkitgtk_4_1 ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libX11 ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libXcursor ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libXfixes ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libXinerama ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.libxcb ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutil ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilimage ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilkeysyms ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.xorg.xcbutilwm ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.fontconfig ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.at-spi2-atk ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.gdk-pixbuf ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.gtkd ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.gtk3.dev ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.gtk3-x11 ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.gtk3-x11.dev ]}"
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"
    export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.wayland ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.libxkbcommon ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.webkitgtk_4_1 ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.gtk3 ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.pango ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.cairo ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.gdk-pixbuf ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.libsoup_3 ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.glib ]}"
    export RUSTFLAGS="$RUSTFLAGS:${pkgs.lib.makeLibraryPath [ pkgs.alsa-lib ]}"
  '';

}
