{ pkgs, ... }: {
  packages = with pkgs; [
    pkg-config
    nodejs
    clang
    trunk
    python312
    fontconfig
    freetype
    xorg.libxcb
    libxkbcommon
    wayland
    vulkan-loader
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.97.1";
    targets = [ "wasm32-unknown-unknown" ];
  };

  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.xorg.libxcb
    pkgs.fontconfig
    pkgs.freetype
    pkgs.libxkbcommon
    pkgs.wayland
    pkgs.vulkan-loader
  ];

  env.CC_wasm32_unknown_unknown = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
}
