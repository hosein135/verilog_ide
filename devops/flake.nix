{
  description = "Verilog IDE development environment (nixpkgs 25.05)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          # xezim (vendored at vendor/xezim) requires rustc 1.92+.
          rustToolchain = pkgs.rust-bin.stable."1.92.0".default;
          linuxRuntimeLibs = pkgs.lib.optionals pkgs.stdenv.isLinux [
            pkgs.libxkbcommon
            pkgs.fontconfig
            pkgs.freetype
            pkgs.dbus
            pkgs.libdrm
            pkgs.systemd
            pkgs.wayland
            pkgs.wayland-protocols
            pkgs.xorg.libX11
            pkgs.xorg.libXext
            pkgs.xorg.libxcb
            pkgs.xorg.libXcursor
            pkgs.xorg.libXi
            pkgs.xorg.libXrandr
            pkgs.xorg.libXfixes
            pkgs.xorg.libXrender
            pkgs.xorg.libxshmfence
          ];
        in
        {
          default = pkgs.mkShell {
            name = "verilog-ide";
            packages = with pkgs; [
              rustToolchain
              pkg-config
              openssl
              libffi
              clang
              llvmPackages.clang
              llvmPackages.libclang
              cmake
              git
              curl
              perl
              zenity
              xdg-desktop-portal-gtk
            ];

            buildInputs = linuxRuntimeLibs
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv pkgs.libffi ];

            shellHook = ''
              export VERILOG_IDE_NIX=1
              export RUST_BACKTRACE=1
              export ICED_BACKEND=tiny-skia
              export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
              export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libffi.dev}/lib/pkgconfig:''${PKG_CONFIG_PATH:-}"
              ${pkgs.lib.optionalString pkgs.stdenv.isLinux ''
                export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath linuxRuntimeLibs}:$LD_LIBRARY_PATH"
              ''}
            '';
          };
        }
      );
    };
}
