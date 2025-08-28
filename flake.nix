{
  description = "MoosicBox - A music app for cows";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config = { };
        };

        # Common packages for all platforms
        commonPackages = with pkgs; [
          pkg-config
          gnumake
          gcc
          libiconv
          autoconf
          automake
          libtool
          cmake
          ninja  # Add ninja for build systems that need it
          openssl
          postgresql
          vips
        ];

        # Linux-specific packages
        linuxPackages = pkgs.lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          alsa-lib
          alsa-lib.dev
          alsa-utils
          udev.dev
          wayland
          wayland-protocols
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
          libGL
          libGLU
          mesa
        ]);

        # macOS-specific packages
        darwinPackages = pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
          portaudio
          libiconv
          # Use Nix-provided clang for better compatibility
          clang
        ]);

        # Rust toolchain from rust-overlay
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };

      in
      {
        devShells = {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [ fontconfig ]
              ++ [ rustToolchain ]
              ++ commonPackages
              ++ linuxPackages
              ++ darwinPackages;

            packages = with pkgs; [
              # Development tools
              cargo-watch
              cargo-edit
              cargo-audit
            ];

            shellHook = ''
              echo "🎵 MoosicBox Development Environment"
              echo "Platform: ${system}"
              echo "Rust: $(rustc --version)"
              echo "Cargo: $(cargo --version)"
              echo ""

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                # Linux-specific environment setup
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath (with pkgs; [
                  wayland libxkbcommon webkitgtk_4_1
                  xorg.libX11 xorg.libXcursor xorg.libXfixes
                  xorg.libXinerama xorg.libxcb xorg.xcbutil
                  xorg.xcbutilimage xorg.xcbutilkeysyms xorg.xcbutilwm
                  fontconfig at-spi2-atk gdk-pixbuf gtkd
                  gtk3.dev gtk3-x11 gtk3-x11.dev openssl
                ])}"

                export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,:${pkgs.lib.makeLibraryPath (with pkgs; [
                  wayland libxkbcommon webkitgtk_4_1 gtk3
                  vulkan-loader pango cairo gdk-pixbuf
                  libsoup_3 glib alsa-lib
                ])}"

                echo "LD_LIBRARY_PATH configured for Linux GUI libraries"
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                # macOS-specific environment setup
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
                export DYLD_LIBRARY_PATH="$DYLD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"
                export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}"

                echo "Using Nix-provided clang for macOS compilation"
              ''}

              echo "Ready for development! Try: cargo build"
            '';
          };

          # Minimal shell for CI/testing environments
          ci = pkgs.mkShell {
            buildInputs = [ rustToolchain ] ++ commonPackages;

            shellHook = ''
              echo "MoosicBox CI Environment"
              echo "Rust: $(rustc --version)"
            '';
          };
        };

        # Optional: Package definition for building MoosicBox
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moosicbox";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildInputs = commonPackages ++ linuxPackages ++ darwinPackages;

          # Skip tests during package build (they may require additional setup)
          doCheck = false;
        };
      });
}
