{ pkgs, lib, ... }: {
  packages = with pkgs; [
    pkg-config
    nodejs
    clang
    trunk
    python312
  ] ++ lib.optionals stdenv.hostPlatform.isLinux [
    gtk4
    qt6.qtbase
    fontconfig
    freetype
    libxcb
    libxkbcommon
    wayland
    vulkan-loader
  ];

  enterShell = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
    unset NIX_CFLAGS_COMPILE NIX_LDFLAGS
  '';

  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.97.1";
    targets = [ "wasm32-unknown-unknown" ];
  };

  env = lib.mkMerge [
    (lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      LD_LIBRARY_PATH = lib.makeLibraryPath [
        pkgs.gtk4
        pkgs.qt6.qtbase
        pkgs.fontconfig
        pkgs.freetype
        pkgs.libxcb
        pkgs.libxkbcommon
        pkgs.wayland
        pkgs.vulkan-loader
      ];
    })
    {
      CC_wasm32_unknown_unknown = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
    }
  ];
}
