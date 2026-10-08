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

  scripts.terminal-prototype = {
    description = "Run Agentaps with the experimental Ghostty session terminal";
    exec = let
      terminalLibraries = lib.makeLibraryPath (lib.optionals pkgs.stdenv.hostPlatform.isLinux [
        pkgs.llvmPackages.libcxx
        pkgs.libxml2
        pkgs.libglvnd
      ]);
    in ''
      export ZIG="${pkgs.zig_0_16}/bin/zig"
      ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
        export LIBRARY_PATH="${terminalLibraries}''${LIBRARY_PATH:+:$LIBRARY_PATH}"
        export LD_LIBRARY_PATH="${terminalLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      ''}
      exec cargo run --features ghostty-terminal "$@"
    '';
  };

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
