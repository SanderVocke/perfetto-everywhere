{
  description = "perfetto-everywhere development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };
      rust = pkgs.rust-bin.stable."1.90.0".default.override {
        extensions = [ "clippy" "rustfmt" "rust-src" ];
        targets = [ "wasm32-unknown-unknown" ];
      };
    in {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          rust
          cargo-audit
          clang
          cmake
          ninja
          pkg-config
          protobuf
          nodejs_22
          python3
          chromium
          curl
          git
          gh
          time
          wasm-bindgen-cli
        ];
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        CHROME_BIN = "${pkgs.chromium}/bin/chromium";
      };
    };
}
