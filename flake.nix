{
  description = "CONTRA: a Kontakt-compatible sampler plugin (CLAP, VST3) for NixOS";

  # Everything the plugin links and loads comes from NixOS stable: a plugin
  # runs inside the DAW's process and must not need a newer glibc than the
  # DAW's. Code built against stable's glibc loads on stable and unstable
  # alike, never the other way round. Only the Rust compiler comes from
  # unstable (1.99, the version rust-toolchain.toml pins; stable has 1.95);
  # it links with stable's toolchain, and Rust's standard library asks for
  # no recent glibc.
  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-26.05/nixexprs.tar.xz";
    nixpkgs-unstable.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";
  };

  outputs = { self, nixpkgs, nixpkgs-unstable }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      unstable = nixpkgs-unstable.legacyPackages.${system};
      lib = pkgs.lib;
      rust = { inherit (unstable) rustc cargo clippy rustfmt rust-analyzer; };
      rustPlatform = pkgs.makeRustPlatform { inherit (rust) rustc cargo; };

      # What the editor opens at run time with dlopen: the Vulkan loader
      # (which finds the system's drivers in /run/opengl-driver), EGL/GL,
      # and the X11 and Wayland client libraries. A DAW on NixOS has none of
      # them on a default search path, so the plugin carries their store
      # paths in its RPATH and loads them in any host.
      runtimeLibs = with pkgs; [
        vulkan-loader
        libglvnd
        libx11
        libxcb
        libxcursor
        libxext
        libxi
        libxrandr
        libxrender
        libxkbcommon
        wayland
      ];

      contra = rustPlatform.buildRustPackage {
        pname = "contra";
        version = "3.0.0-dev";
        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let rel = lib.removePrefix (toString ./. + "/") (toString path);
            in !(lib.hasPrefix "target" rel || lib.hasPrefix "docs" rel || lib.hasPrefix "result" rel);
        };
        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [ pkg-config patchelf ];
        buildInputs = runtimeLibs ++ [ pkgs.alsa-lib ];

        cargoBuildFlags = [ "-p" "contra-plugin" ];
        # The full suite runs in the dev shell (`cargo test --workspace`);
        # the package build only builds.
        doCheck = false;

        installPhase = ''
          runHook preInstall
          so=$(find target -path '*/release/libcontra_plugin.so' | head -n1)

          mkdir -p $out/lib/clap $out/lib/vst3/CONTRA.vst3/Contents/x86_64-linux
          install -m755 "$so" $out/lib/clap/CONTRA.clap
          install -m755 "$so" $out/lib/vst3/CONTRA.vst3/Contents/x86_64-linux/CONTRA.so
          runHook postInstall
        '';

        # After fixup: its RPATH shrinking keeps only directly linked
        # libraries and would drop every dlopen'd one.
        dontPatchELF = true;
        postFixup = ''
          for f in $out/lib/clap/CONTRA.clap $out/lib/vst3/CONTRA.vst3/Contents/x86_64-linux/CONTRA.so; do
            patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} "$f"
          done
        '';

        meta = {
          description = "CONTRA sampler plugin (CLAP and VST3)";
          license = lib.licenses.asl20;
          platforms = [ "x86_64-linux" ];
        };
      };
      # A bare CLAP host that opens the editor in an X11 window and
      # screenshots it: the UI check that needs no DAW (tools/clap-probe).
      clap-probe = rustPlatform.buildRustPackage {
        pname = "clap-probe";
        version = "0.0.0";
        inherit (contra) src;
        cargoLock.lockFile = ./Cargo.lock;
        cargoBuildFlags = [ "-p" "clap-probe" ];
        doCheck = false;
      };

      # The same check for VST3 (tools/vst3-probe): a bare host that serves
      # IPlugFrame and IRunLoop and attaches the editor to an X11 window.
      vst3-probe = rustPlatform.buildRustPackage {
        pname = "vst3-probe";
        version = "0.0.0";
        inherit (contra) src;
        cargoLock.lockFile = ./Cargo.lock;
        cargoBuildFlags = [ "-p" "vst3-probe" ];
        doCheck = false;
      };
    in
    {
      packages.${system} = {
        default = contra;
        inherit clap-probe vst3-probe;
      };

      # `nix flake check`: each format's editor opens in a bare host under
      # Xvfb, with Mesa's software Vulkan as the only driver, and must
      # render through the GPU path (a CPU fallback means the plugin could
      # not reach the graphics libraries) and draw. The screenshots land in
      # the check's output.
      checks.${system} =
        let
          uiCheck = name: probe: plugin: pkgs.runCommand "contra-ui-${name}" {
            nativeBuildInputs = [ pkgs.xvfb-run ];
          } ''
            export HOME=$TMPDIR
            export VK_DRIVER_FILES=${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json
            export __EGL_VENDOR_LIBRARY_FILENAMES=${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json
            mkdir -p $out
            xvfb-run -s "-screen 0 1920x1200x24" \
              ${probe}/bin/${probe.pname} ${plugin} $out/editor.png 60 2>&1 | tee $out/log
            grep -q "GPU ready backend=Vulkan" $out/log
            grep -q "closed cleanly" $out/log
          '';
        in
        {
          ui-clap = uiCheck "clap" clap-probe "${contra}/lib/clap/CONTRA.clap";
          ui-vst3 = uiCheck "vst3" vst3-probe "${contra}/lib/vst3/CONTRA.vst3";
        };

      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ contra ];
        packages = (builtins.attrValues rust) ++ (with pkgs; [ xvfb-run mesa gnumake patchelf ]);
        LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
        # What `make clap` / `make vst3` bake into the plugin, as the package does.
        CONTRA_RPATH = lib.makeLibraryPath runtimeLibs;
        # Mesa's software Vulkan, for GPU tests on a machine without a GPU
        # (CI): `VK_DRIVER_FILES=$CONTRA_SOFTWARE_VULKAN cargo test ...`.
        CONTRA_SOFTWARE_VULKAN = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";
      };
    };
}
