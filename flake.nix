{
  description = "nstt";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    crane,
    fenix,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};

      toolchain = fenix.packages.${system}.stable.toolchain;

      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
      src = craneLib.cleanCargoSource ./.;

      buildInputs = with pkgs; [
        gtk4
        gtk4-layer-shell
        glib
        pango
        gdk-pixbuf
        wayland
        wayland-protocols
        dbus
      ];

      nativeBuildInputs = with pkgs; [
        pkg-config
        makeWrapper
        wrapGAppsHook4
      ];

      envVars = {
        RUST_BACKTRACE = "full";
      };

      cargoArtifacts = craneLib.buildDepsOnly {
        inherit src buildInputs nativeBuildInputs;
        env = envVars;
      };

      nstt = craneLib.buildPackage {
        inherit src cargoArtifacts buildInputs nativeBuildInputs;
        env = envVars;
        pname = "nstt";
        version = "0.1.0";
        postInstall = ''
          install -d $out/share/glib-2.0/schemas
          cat > $out/share/glib-2.0/schemas/github.niahex.nstt.gschema.xml << EOF
          <?xml version="1.0" encoding="UTF-8"?>
          <schemalist>
            <schema id="github.niahex.nstt" path="/github/niahex/nstt/">
              <!-- Aucune clé de schéma pour le moment -->
            </schema>
          </schemalist>
          EOF
        '';
      };
    in {
      packages = {
        default = nstt;
        nstt = nstt;
      };

      checks = {
        inherit nstt;

        nstt-clippy = craneLib.cargoClippy {
          inherit src cargoArtifacts buildInputs nativeBuildInputs;
          env = envVars;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        };

        nstt-fmt = craneLib.cargoFmt {
          inherit src;
        };
      };

      devShells.default = pkgs.mkShell {
        inputsFrom = [nstt];
        nativeBuildInputs = with pkgs; [
          fenix.packages.${system}.rust-analyzer
          fenix.packages.${system}.stable.toolchain
          cargo-watch
          cargo-edit
          bacon
          nerd-fonts.ubuntu-mono
          nerd-fonts.ubuntu-sans
          nerd-fonts.ubuntu
        ];

        env = envVars;

        shellHook = ''
          echo "[🦀 Rust $(rustc --version)] - Ready !"
          echo "Dépendances: ${pkgs.lib.concatStringsSep " " (map (p: p.name) nativeBuildInputs)}"
          echo "Available commands: cargo watch, cargo edit, bacon"
        '';
      };

      formatter = pkgs.alejandra;
    });
}
