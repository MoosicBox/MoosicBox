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

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
        };

        # Rust toolchain from rust-overlay
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
            "llvm-tools-preview"
          ];
        };

        rustToolchainNightly = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "llvm-tools-preview" ];
        };

        # ===== BASE PACKAGE SETS =====

        # Minimal build tools (base for all shells)
        baseBuildTools = with pkgs; [
          pkg-config
          gnumake
          gcc
          libiconv
          autoconf
          automake
          libtool
          cmake
          ninja
          openssl
          lua5_4 # Required for mlua crate with lua54 feature
        ];

        # Audio packages (common to many apps)
        audioPackages =
          with pkgs;
          [
            libopus # Opus codec library
            opusTools # Opus codec tools (opusenc, opusdec, opusinfo)
            portaudio # Cross-platform audio (wraps CoreAudio on macOS, ALSA on Linux)
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            alsa-lib
            alsa-lib.dev
            alsa-utils
            pulseaudio
            pipewire
            jack2
          ];

        # Android SDK packages (for mobile development)
        androidPackages = pkgs.androidenv.composeAndroidPackages {
          cmdLineToolsVersion = "13.0";
          toolsVersion = "26.1.1";
          platformToolsVersion = "35.0.2";
          buildToolsVersions = [
            "30.0.3"
            "34.0.0"
          ];
          platformVersions = [
            "33"
            "34"
          ];
          includeEmulator = false;
          includeSystemImages = false;
          systemImageTypes = [ "default" ];
          abiVersions = [
            "arm64-v8a"
            "armeabi-v7a"
            "x86"
            "x86_64"
          ];
          includeSources = false;
          includeNDK = true;
          useGoogleAPIs = false;
          useGoogleTVAddOns = false;
          includeExtras = [ ];
          extraLicenses = [ ];
        };

        # ===== GUI BACKEND-SPECIFIC PACKAGES =====

        # GTK/WebKit packages (for GTK-based apps and Tauri)
        gtkPackages =
          with pkgs;
          [
            # Cross-platform GTK packages
            gtk3
            gtk3.dev
            glib
            cairo
            pango
            gdk-pixbuf
            at-spi2-atk
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            # Linux-specific GTK packages
            gtk3-x11
            gtk3-x11.dev
            gtkd
            webkitgtk_4_1
            libsoup_3
            gst_all_1.gstreamer
            gst_all_1.gst-plugins-base
            gst_all_1.gst-plugins-good
            gst_all_1.gst-plugins-bad
            gsettings-desktop-schemas
          ];

        # FLTK-specific packages
        fltkPackages =
          with pkgs;
          [
            fltk
            fontconfig
            freetype
            cairo
            pango
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            # X11 and OpenGL packages (Linux-specific)
            xorg.libX11
            xorg.libXcursor
            xorg.libXfixes
            xorg.libXinerama
            xorg.libXft
            xorg.libXext
            xorg.libXrender
            libGL
            libGLU
            mesa
          ];

        # Egui/wgpu packages (for egui-based apps)
        eguiPackages =
          with pkgs;
          [
            # Cross-platform graphics packages
            vulkan-loader
            vulkan-headers
            vulkan-validation-layers
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            # Linux-specific display and graphics packages
            xorg.libX11
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXi
            libGL
            mesa
            wayland
            wayland-protocols
            libxkbcommon
          ];

        # Wayland/X11 base packages (Linux-only display servers)
        displayServerPackages = pkgs.lib.optionals pkgs.stdenv.isLinux (
          with pkgs;
          [
            xorg.libX11
            xorg.libxcb
            xorg.xcbutil
            xorg.xcbutilimage
            xorg.xcbutilkeysyms
            xorg.xcbutilwm
            xorg.libXinerama
            wayland
            wayland-protocols
            libxkbcommon
          ]
        );

        # ===== SHELL BUILDERS =====

        # Basic shell for non-GUI components
        mkBasicShell =
          {
            name,
            packages ? [ ],
          }:
          pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ packages;
            shellHook = ''
              echo "🎵 MoosicBox ${name} Environment"
              echo "Rust: $(rustc --version)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

        # GTK-based GUI shell
        mkGtkShell =
          {
            name,
            extraPackages ? [ ],
          }:
          pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ audioPackages
            ++ displayServerPackages
            ++ gtkPackages
            ++ extraPackages
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.clang ];

            shellHook = ''
              echo "🎵 MoosicBox ${name} Environment (GTK Backend)"
              echo "Rust: $(rustc --version)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
                  pkgs.lib.makeLibraryPath (gtkPackages ++ displayServerPackages)
                }"
                export GDK_BACKEND=x11,wayland
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
              ''}

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

        # FLTK-based GUI shell
        mkFltkShell =
          {
            name,
            extraPackages ? [ ],
          }:
          pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ audioPackages
            ++ fltkPackages
            ++ extraPackages
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.clang ];

            shellHook = ''
              echo "🎵 MoosicBox ${name} Environment (FLTK Backend)"
              echo "Rust: $(rustc --version)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath fltkPackages}"
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
              ''}

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

        # Egui-based GUI shell
        mkEguiShell =
          {
            name,
            extraPackages ? [ ],
          }:
          pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ audioPackages
            ++ eguiPackages
            ++ extraPackages
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.clang
              pkgs.darwin.apple_sdk.frameworks.Metal
              pkgs.darwin.apple_sdk.frameworks.MetalKit
            ];

            shellHook = ''
              echo "🎵 MoosicBox ${name} Environment (Egui/WGPU Backend)"
              echo "Rust: $(rustc --version)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath eguiPackages}"
                export VK_ICD_FILENAMES="${pkgs.vulkan-loader}/share/vulkan/icd.d/lvp_icd.x86_64.json"
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
              ''}

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

        # Tauri-based app shell (extends GTK shell with Tauri needs)
        mkTauriShell =
          {
            name,
            extraPackages ? [ ],
          }:
          mkGtkShell {
            name = "Tauri ${name}";
            extraPackages =
              with pkgs;
              [
                # Node.js ecosystem for Tauri development
                nodejs
                nodePackages.pnpm
                # Tauri CLI will be installed via package.json
              ]
              ++ extraPackages;
          };

        # HyperChad web development shell (with Playwright for testing)
        mkHyperchadShell =
          {
            name,
            extraPackages ? [ ],
          }:
          pkgs.mkShell {
            buildInputs = [
              rustToolchain
              pkgs.fish
              pkgs.nodejs
              pkgs.nodePackages.pnpm
              pkgs.playwright
            ]
            ++ baseBuildTools
            ++ extraPackages;

            shellHook = ''
              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              export PLAYWRIGHT_BROWSERS_PATH=${pkgs.playwright.browsers}
              export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true

              echo "🎵 MoosicBox HyperChad ${name} Environment"
              echo "Rust: $(rustc --version)"
              echo "Node: $(node --version)"
              echo "Playwright browsers: $PLAYWRIGHT_BROWSERS_PATH"

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

      in
      {
        devShells = {
          # ===== MAIN SHELLS =====
          default = pkgs.mkShell {
            # Kitchen sink environment with everything
            buildInputs = [
              rustToolchain
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ audioPackages
            ++ displayServerPackages
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux (gtkPackages ++ fltkPackages ++ eguiPackages)
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.clang
              pkgs.portaudio
            ];

            packages = with pkgs; [
              cargo-watch
              cargo-edit
              cargo-audit
              postgresql
              vips
              yq-go # For YAML parsing in reproduce scripts
            ];

            shellHook = ''
              echo "🎵 MoosicBox Full Development Environment"
              echo "Platform: ${system}"
              echo "Rust: $(rustc --version)"
              echo ""
              echo "Available environments:"
              echo "  Server: .#server, .#tunnel-server"
              echo "  Coverage: .#coverage (nightly with llvm-tools-preview)"
              echo "  Tauri: .#tauri-solidjs, .#tauri-hyperchad-fltk, .#tauri-hyperchad-egui"
              echo "  Tauri Bundled: .#tauri-solidjs-bundled, .#tauri-hyperchad-fltk-bundled, .#tauri-hyperchad-egui-bundled"
              echo "  GUI: .#fltk-*, .#egui-*, .#gtk-*"
              echo "  Android: .#android (compose with Tauri shells)"
              echo "  Full: .#tauri-full (all Tauri variants)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
                  pkgs.lib.makeLibraryPath (gtkPackages ++ fltkPackages ++ eguiPackages ++ displayServerPackages)
                }"
                export GDK_BACKEND=x11,wayland
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
              ''}

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

          ci = mkBasicShell {
            name = "CI";
            packages = [ ];
          };

          coverage = pkgs.mkShell {
            name = "Coverage Testing";
            buildInputs = [
              rustToolchainNightly # Nightly for llvm-tools-preview
              pkgs.fish
            ]
            ++ baseBuildTools
            ++ audioPackages
            ++ displayServerPackages
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux (gtkPackages ++ fltkPackages ++ eguiPackages)
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.clang
              pkgs.portaudio
            ];

            packages = with pkgs; [
              cargo-watch
              cargo-edit
              cargo-audit
              postgresql
              vips
            ];

            shellHook = ''
              echo "🎵 MoosicBox Coverage Environment (Nightly)"
              echo "Rust: $(rustc --version)"
              echo ""
              echo "This shell uses nightly Rust with llvm-tools-preview for coverage."
              echo "All dependencies from the default shell are included."
              echo ""
              echo "Run coverage with: cargo llvm-cov"
              echo "Generate HTML report: cargo llvm-cov --html"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
                  pkgs.lib.makeLibraryPath (gtkPackages ++ fltkPackages ++ eguiPackages ++ displayServerPackages)
                }"
                export GDK_BACKEND=x11,wayland
              ''}

              ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                export CC="${pkgs.clang}/bin/clang"
                export CXX="${pkgs.clang}/bin/clang++"
              ''}

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

          # ===== SERVER COMPONENTS =====

          server = mkBasicShell {
            name = "Server";
            packages = with pkgs; [
              postgresql
              sqlite
              llvmPackages.libclang
              glibc.dev
            ];
          };

          tunnel-server = mkBasicShell {
            name = "Tunnel Server";
            packages = [ ];
          };

          # ===== GTK-BASED APPLICATIONS =====

          gtk-marketing-site = mkGtkShell {
            name = "Marketing Site";
            extraPackages = with pkgs; [ vips ];
          };

          # ===== TAURI-BASED APPLICATIONS =====

          # Base Tauri variants (external server)
          tauri-solidjs = mkTauriShell {
            name = "SolidJS";
            extraPackages = [ ];
          };

          tauri-hyperchad-fltk = mkTauriShell {
            name = "HyperChad FLTK";
            extraPackages = fltkPackages;
          };

          tauri-hyperchad-egui = mkTauriShell {
            name = "HyperChad Egui";
            extraPackages = eguiPackages;
          };

          # Bundled variants (with embedded server)
          tauri-solidjs-bundled = mkTauriShell {
            name = "SolidJS Bundled";
            extraPackages = with pkgs; [
              postgresql
              sqlite
              vips
            ];
          };

          tauri-hyperchad-fltk-bundled = mkTauriShell {
            name = "HyperChad FLTK Bundled";
            extraPackages =
              fltkPackages
              ++ (with pkgs; [
                postgresql
                sqlite
                vips
              ]);
          };

          tauri-hyperchad-egui-bundled = mkTauriShell {
            name = "HyperChad Egui Bundled";
            extraPackages =
              eguiPackages
              ++ (with pkgs; [
                postgresql
                sqlite
                vips
              ]);
          };

          # Full Tauri development (everything)
          tauri-full = mkTauriShell {
            name = "Full Development";
            extraPackages =
              fltkPackages
              ++ eguiPackages
              ++ (with pkgs; [
                postgresql
                sqlite
                vips
                # Additional dev tools
                cargo-watch
                cargo-edit
                cargo-audit
              ]);
          };

          # ===== FLTK-BASED APPLICATIONS =====

          fltk-renderer = mkFltkShell {
            name = "FLTK Renderer";
            extraPackages = with pkgs; [ udev.dev ];
          };

          fltk-hyperchad = mkFltkShell {
            name = "Hyperchad FLTK";
            extraPackages = [ ];
          };

          # ===== EGUI-BASED APPLICATIONS =====

          egui-native = mkEguiShell {
            name = "Native App";
            extraPackages =
              with pkgs;
              [
                vulkan-loader
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ amdvlk ];
          };

          egui-player = mkEguiShell {
            name = "Egui Player";
            extraPackages = [ ];
          };

          # ===== HYPERCHAD DEVELOPMENT =====

          hyperchad-web = mkHyperchadShell {
            name = "Web";
            extraPackages = [ ];
          };

          # ===== ANDROID DEVELOPMENT =====

          android = pkgs.mkShell {
            buildInputs = [
              androidPackages.androidsdk
              pkgs.jdk17
              pkgs.gradle
              pkgs.fish
            ];

            shellHook = ''
              echo "📱 Android SDK Environment"
              echo "Java: $(java --version | head -1)"

              export TUNNEL_ACCESS_TOKEN=123
              export STATIC_TOKEN=123

              export ANDROID_HOME="${androidPackages.androidsdk}/libexec/android-sdk"
              export ANDROID_SDK_ROOT="$ANDROID_HOME"
              export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk-bundle"
              export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/tools:$ANDROID_HOME/tools/bin:$PATH"

              # Install Android targets for Rust if rustup is available
              if command -v rustup &> /dev/null; then
                echo "Installing Rust Android targets..."
                rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android 2>/dev/null || true
              fi

              echo ""
              echo "Android SDK: $ANDROID_HOME"
              echo "Available: adb, gradle, fastboot"
              echo ""
              echo "For Android development, first enter this shell:"
              echo "  nix develop .#android"
              echo "Then in a separate terminal for Tauri:"
              echo "  nix develop .#tauri-solidjs"
              echo "Or create a combined shell for your specific use case."

              # Only exec fish if we're in an interactive shell (not running a command)
              if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
                case "$-" in
                  *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
                esac
              fi
            '';
          };

        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moosicbox";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildInputs = baseBuildTools ++ audioPackages;
          doCheck = false;
        };
      }
    );
}
