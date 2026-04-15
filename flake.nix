{
  description = "Aileron - Keyboard-driven tiling web environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      libPath = with pkgs; lib.makeLibraryPath[
        libGL
        libxkbcommon
        wayland
        xorg.libX11
        xorg.libXcursor
        xorg.libXi
        xorg.libXrandr
        fontconfig
        freetype
        openssl
        webkitgtk_4_1
        gtk3
        glib
        pango
        cairo
        atk
        gdk-pixbuf
        libsoup_3
      ];
      pkgConfigPath = with pkgs; lib.makeSearchPath "lib/pkgconfig" [
        webkitgtk_4_1
        gtk3
        glib
        pango
        cairo
        atk
        gdk-pixbuf
        libsoup_3
      ] + ":" + lib.makeSearchPath "share/pkgconfig" [
        gtk3
        glib
        pango
        cairo
        atk
        gdk-pixbuf
      ];

      # The aileron package: built with a wrapper that sets runtime env vars
      aileron-package = pkgs.rustPlatform.buildRustPackage {
        pname = "aileron";
        version = "0.1.0";

        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        nativeBuildInputs = with pkgs; [
          pkg-config
          cmake
          python3
          gnumake
        ];

        buildInputs = with pkgs; [
          libGL
          libxkbcommon
          wayland
          vulkan-loader
          fontconfig
          freetype
          openssl
          webkitgtk_4_1
          gtk3
          glib
          pango
          cairo
          atk
          gdk-pixbuf
          libsoup_3
        ];

        # Make sure pkg-config can find all the libraries
        PKG_CONFIG_PATH = pkgConfigPath;

        # Runtime wrapper: sets LD_LIBRARY_PATH + VK_ICD_FILENAMES + WINIT backend
        postInstall = ''
          mkdir -p $out/bin
          cat > $out/bin/aileron <<'EOF'
          #!/usr/bin/env bash
          set -e

          # Runtime library path
          export LD_LIBRARY_PATH="${libPath}:$LD_LIBRARY_PATH"

          # Wayland + X11 fallback
          export WINIT_UNIX_BACKEND="wayland,x11"

          # Point Vulkan to system GPU drivers
          if [ -f /usr/share/vulkan/icd.d/nvidia_icd.json ]; then
            export VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/nvidia_icd.json"
          elif [ -f /usr/share/vulkan/icd.d/intel_icd.i686.json ]; then
            export VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/intel_icd.i686.json"
          elif [ -f /usr/share/vulkan/icd.d/radeon_icd.x86_64.json ]; then
            export VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/radeon_icd.x86_64.json"
          fi

          exec "$out/bin/.aileron-wrapped" "$@"
          EOF
          chmod +x $out/bin/aileron

          # Move the actual binary to be wrapped
          mv $out/bin/aileron $out/bin/.aileron-wrapped

          # Install desktop entry and icon
          install -Dm644 ${./resources/aileron.desktop} $out/share/applications/aileron.desktop
          install -Dm644 ${./resources/aileron.svg} $out/share/icons/hicolor/scalable/apps/aileron.svg
        '';
      };
    in
    {
      packages.${system}.default = aileron-package;

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs;[
          # Rust Toolchain (use system rustc for faster startup)
          cargo
          rustc
          rustfmt
          clippy

          # Build tools required by WGPU & wry
          pkg-config
          cmake
          python3
          gnumake

          # GUI / Graphics / Wayland
          libxkbcommon
          wayland
          vulkan-loader

          # Fonts and SSL
          fontconfig
          freetype
          openssl

          # wry (Tauri's WebView) dependencies — WebKitGTK on Linux
          webkitgtk_4_1
          gtk3
          glib
          pango
          cairo
          atk
          gdk-pixbuf
          libsoup_3
        ];

        shellHook = ''
          export LD_LIBRARY_PATH="${libPath}:$LD_LIBRARY_PATH"
          export PKG_CONFIG_PATH="${pkgConfigPath}:$PKG_CONFIG_PATH"
          export WINIT_UNIX_BACKEND=wayland,x11
          export RUST_LOG="info"
          # Point Vulkan to system GPU drivers (NVIDIA)
          export VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/nvidia_icd.json"

          echo "✈️  Welcome to the Aileron dev environment (CachyOS/Wayland/Vulkan ready)"
          echo "  Build:  cargo build"
          echo "  Test:   cargo test --lib -- --test-threads=4"
          echo "  Run:    cargo run"
          echo ""
          echo "  Or install with: nix build && ./result/bin/aileron"
        '';
      };
    };
}
