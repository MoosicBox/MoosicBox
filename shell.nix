let
  pkgs = import <nixpkgs> {
    config = { };
    overlays = [ ];
  };

  # Common packages that work on all platforms
  commonPackages = with pkgs; [
    pkg-config
    gnumake
    gcc
    libiconv
    autoconf
    automake
    libtool
    cmake
    openssl
    postgresql
    vips
  ];

  # Linux-specific packages
  linuxPackages = with pkgs; [
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
    gtkd
    gtk3.dev
    gtk3-x11
    gtk3-x11.dev
    gsettings-desktop-schemas
    pulseaudio
  ];

  # macOS-specific packages
  darwinPackages = with pkgs; [
    darwin.apple_sdk.frameworks.CoreAudio
    darwin.apple_sdk.frameworks.AudioUnit
    darwin.apple_sdk.frameworks.AudioToolbox
    darwin.apple_sdk.frameworks.CoreFoundation
    darwin.apple_sdk.frameworks.CoreServices
    darwin.apple_sdk.frameworks.Security
    darwin.apple_sdk.frameworks.SystemConfiguration
    portaudio
  ];

  # Platform-specific shell hooks
  linuxShellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
      pkgs.lib.makeLibraryPath [
        pkgs.wayland
        pkgs.libxkbcommon
        pkgs.webkitgtk_4_1
        pkgs.xorg.libX11
        pkgs.xorg.libXcursor
        pkgs.xorg.libXfixes
        pkgs.xorg.libXinerama
        pkgs.xorg.libxcb
        pkgs.xorg.xcbutil
        pkgs.xorg.xcbutilimage
        pkgs.xorg.xcbutilkeysyms
        pkgs.xorg.xcbutilwm
        pkgs.fontconfig
        pkgs.at-spi2-atk
        pkgs.gdk-pixbuf
        pkgs.gtkd
        pkgs.gtk3.dev
        pkgs.gtk3-x11
        pkgs.gtk3-x11.dev
        pkgs.openssl
      ]
    }"
    export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,:${
      pkgs.lib.makeLibraryPath [
        pkgs.wayland
        pkgs.libxkbcommon
        pkgs.webkitgtk_4_1
        pkgs.gtk3
        pkgs.vulkan-loader
        pkgs.pango
        pkgs.cairo
        pkgs.gdk-pixbuf
        pkgs.libsoup_3
        pkgs.glib
        pkgs.alsa-lib
      ]
    }"
  '';

  darwinShellHook = ''
    export DYLD_LIBRARY_PATH="$DYLD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"
    export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"
  '';

in

pkgs.mkShellNoCC {
  buildInputs = with pkgs; [
    fontconfig
  ];
  packages =
    commonPackages
    ++ (
      if pkgs.stdenv.isDarwin then
        darwinPackages
      else if pkgs.stdenv.isLinux then
        linuxPackages
      else
        [ ]
    );

  shellHook =
    if pkgs.stdenv.isDarwin then
      darwinShellHook
    else if pkgs.stdenv.isLinux then
      linuxShellHook
    else
      "";
}
