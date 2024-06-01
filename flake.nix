{
  description = "Audio thing";

  inputs = {
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs.outPath {
          config = { allowUnfree = true; };
          inherit system;
        };
      in
      {
        devShells.check = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Nix
            nixpkgs-fmt

            # Rust
            rustc
            cargo
            clippy
            rustfmt
            pkg-config
            alsa-lib
            libjack2

            # Misc
            nushell
            just
            nodePackages.prettier
          ];
        };

        devShells.build = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust
            rustc
            cargo

            # Libraries
            pkg-config
            alsa-lib
            libjack2
            vulkan-loader
            wayland
            wayland-protocols
            libxkbcommon

            # Misc
            nushell
            just
          ];
        };

        devShells.docs = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust
            rustc
            cargo

            # Libraries
            pkg-config
            alsa-lib
            libjack2
            vulkan-loader
            wayland
            wayland-protocols
            libxkbcommon

            # Misc
            nushell
            just
          ];
        };

        devShells.default = pkgs.mkShell rec {
          buildInputs = with pkgs; [
            # Nix
            nil
            nixpkgs-fmt

            # Rust
            llvmPackages.clangNoLibcxx
            llvmPackages.lldb
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            cargo-edit
            evcxr

            # Libraries
            pkg-config
            alsa-lib
            libjack2
            vulkan-loader
            wayland
            wayland-protocols
            libxkbcommon

            # Misc
            simple-http-server
            nushell
            just
            nodePackages.prettier
            nodePackages.yaml-language-server
            nodePackages.vscode-json-languageserver
            marksman
            taplo
          ];

          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
          RUST_BACKTRACE = "full";
        };
      }
    );
}
