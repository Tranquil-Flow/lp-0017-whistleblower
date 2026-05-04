{
  description = "LP-0017 Whistleblower — LEZ registry program + reusable indexing module + Basecamp UI plugin";

  inputs = {
    # Follow logos-workspace's pinned nixpkgs so Qt versions match Basecamp.
    logos-nix.url = "github:logos-co/logos-nix";
    nixpkgs.follows = "logos-nix/nixpkgs";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    nix-bundle-lgx = {
      url = "github:logos-co/nix-bundle-lgx";
      inputs.logos-nix.follows = "logos-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, logos-nix, rust-overlay, flake-utils, nix-bundle-lgx }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # ── ZK circuit artifacts (needed by logos-blockchain-pol build.rs) ────
        # Per-arch sha256s captured via `nix-prefetch-url`. Linux hash is
        # the value whisper-wall uses; aarch64-darwin captured fresh.
        circuitsHashes = {
          "x86_64-linux"   = "13c5gkfsa70kca0nwffbsis2difmspyk8aqmlzhq12mhr3x1y4z9";
          "aarch64-darwin" = "1algaks0s3ylm5pvxd8b35nncdhnskvh9fzphn5b90cx6cj0h035";
        };
        circuitsArch = {
          "x86_64-linux"   = "linux-x86_64";
          "aarch64-darwin" = "macos-aarch64";
        };
        logosCiruits = pkgs.fetchurl {
          url = "https://github.com/logos-blockchain/logos-blockchain-circuits/releases/download/v0.4.2/logos-blockchain-circuits-v0.4.2-${circuitsArch.${system} or "linux-x86_64"}.tar.gz";
          # Use TOFU on first build — replace this hash with the printed value.
          sha256 = circuitsHashes.${system} or "13c5gkfsa70kca0nwffbsis2difmspyk8aqmlzhq12mhr3x1y4z9";
        };

        circuitsDir = pkgs.runCommand "logos-blockchain-circuits" {} ''
          mkdir -p $out
          tar -xzf ${logosCiruits} -C $out --strip-components=1
        '';

        # ── LEZ source (for nssa build.rs artifacts) ─────────────────────────
        lezSrc = pkgs.fetchgit {
          url = "https://github.com/logos-blockchain/logos-execution-zone.git";
          rev = "35d8df0d031315219f94d1546ceb862b0e5b208f";
          hash = "sha256-j0DzDvH88IUIReYi6N4FD6+mTIJOklQjaa9qjw4yHEg=";
        };

        # ── Rust FFI cdylib ──────────────────────────────────────────────────
        # `src` is the whole workspace because the FFI has path deps on
        # core/, indexing/, adapters/lez/. cargoBuildFlags scopes the build
        # to just the FFI crate so we don't compile the guest etc.
        ffi = rustPlatform.buildRustPackage {
          pname = "whistleblower-ffi";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            # Starting set copied from whisper-wall — they share most LEZ
            # transitive deps. Will need updating on first nix build (nix
            # will print the correct hash for any mismatched entries).
            outputHashes = {
              "amm_core-0.1.0"                          = "sha256-j0DzDvH88IUIReYi6N4FD6+mTIJOklQjaa9qjw4yHEg=";
              "jf-crhf-0.1.1"                           = "sha256-TUm91XROmUfqwFqkDmQEKyT9cOo1ZgAbuTDyEfe6ltg=";
              "jf-poseidon2-0.1.0"                      = "sha256-QeCjgZXO7lFzF2Gzm2f8XI08djm5jyKI6D8U0jNTPB8=";
              "logos-blockchain-blend-crypto-0.1.2"     = "sha256-8u4P4yDkxrHzQKZLtxl+orQjJCP55CCIxQZ1V2Lbruc=";
              "overwatch-0.1.0"                         = "sha256-L7R1GdhRNNsymYe3RVyYLAmd6x1YY08TBJp4hG4/YwE=";
            };
          };

          cargoBuildFlags = [ "-p" "whistleblower_ffi" ];

          # whistleblower_ffi doesn't depend on the Risc0 guest, but cargo's
          # workspace-wide dep walk still triggers methods/build.rs which
          # invokes risc0-build's `embed_methods` — that panics in the nix
          # sandbox because it tries to write to read-only paths. Setting
          # RISC0_SKIP_BUILD=1 short-circuits embed_methods (it's a documented
          # env var risc0-build honors).
          RISC0_SKIP_BUILD = "1";

          # logos-blockchain-pol build.rs requires ZK circuit artifacts.
          LOGOS_BLOCKCHAIN_CIRCUITS = "${circuitsDir}";

          # nssa build.rs reads ../artifacts/program_methods/*.bin relative to
          # its CARGO_MANIFEST_DIR.
          preBuild = ''
            ln -sf "${lezSrc}/artifacts" ../cargo-vendor-dir/artifacts
          '';

          doCheck = false;
        };

        # ── Qt6 C++ plugin ───────────────────────────────────────────────────
        plugin = pkgs.stdenv.mkDerivation {
          pname = "whistleblower-plugin";
          version = "0.1.0";
          src = ./ui;

          nativeBuildInputs = [
            pkgs.cmake
            pkgs.ninja
            pkgs.pkg-config
            pkgs.qt6.wrapQtAppsHook
          ];

          buildInputs = with pkgs.qt6; [
            qtbase
            qtdeclarative
          ];

          cmakeFlags = [
            "-DWHISTLEBLOWER_FFI_LIB_DIR=${ffi}/lib"
          ];

          installPhase = ''
            runHook preInstall
            cmake --install .
            cp ${./ui/manifest.json} $out/manifest.json
            cp ${./ui/metadata.json} $out/metadata.json
            cp -r ${./ui/qml} $out/qml
            runHook postInstall
          '';
        };

        # ── Install helper ──────────────────────────────────────────────────
        installScript = pkgs.writeShellScriptBin "install-whistleblower-plugin" ''
          PLUGIN_DIR="$HOME/.local/share/Logos/LogosBasecampDev/plugins/whistleblower"
          mkdir -p "$PLUGIN_DIR"
          cp -f ${plugin}/lib/libwhistleblower_plugin.* "$PLUGIN_DIR/" 2>/dev/null || true
          cp -f ${plugin}/lib/libwhistleblower_ffi.*    "$PLUGIN_DIR/" 2>/dev/null || true
          cp -f ${plugin}/manifest.json                  "$PLUGIN_DIR/"
          cp -f ${plugin}/metadata.json                  "$PLUGIN_DIR/"
          echo "Installed to $PLUGIN_DIR"
        '';

        lgx = nix-bundle-lgx.bundlers.${system}.portable plugin;

      in {
        packages = {
          default = plugin;
          ffi     = ffi;
          plugin  = plugin;
          install = installScript;
          lgx     = lgx;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            rustToolchain
            pkgs.cmake pkgs.ninja pkgs.pkg-config
            pkgs.qt6.wrapQtAppsHook
          ];
          buildInputs = with pkgs.qt6; [ qtbase qtdeclarative ];
          shellHook = ''
            echo "whistleblower workspace dev shell"
            echo "  cmake, ninja, Qt6, Rust all on PATH"
            echo "  Build FFI:    cargo build --release -p whistleblower_ffi"
            echo "  Build plugin: cmake -B build -GNinja ./ui && cmake --build build"
            echo "  Build .lgx:   nix build .#lgx"
            echo "  Install plugin (dev): nix run .#install"
          '';
        };
      });
}
