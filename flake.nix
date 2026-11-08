{
  description = "KONTRA: a Kontakt-compatible sampler plugin (CLAP, VST3) for NixOS";

  # nixos-unstable ships rustc 1.99.0, the version rust-toolchain.toml pins.
  inputs.nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      lib = pkgs.lib;

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

      kontra = pkgs.rustPlatform.buildRustPackage {
        pname = "kontra";
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

        cargoBuildFlags = [ "-p" "kontra-plugin" ];
        # The full suite runs in the dev shell (`cargo test --workspace`);
        # the package build only builds.
        doCheck = false;

        installPhase = ''
          runHook preInstall
          so=$(find target -path '*/release/libkontra_plugin.so' | head -n1)

          mkdir -p $out/lib/clap $out/lib/vst3/KONTRA.vst3/Contents/x86_64-linux
          install -m755 "$so" $out/lib/clap/KONTRA.clap
          install -m755 "$so" $out/lib/vst3/KONTRA.vst3/Contents/x86_64-linux/KONTRA.so
          runHook postInstall
        '';

        # After fixup: its RPATH shrinking keeps only directly linked
        # libraries and would drop every dlopen'd one.
        dontPatchELF = true;
        postFixup = ''
          for f in $out/lib/clap/KONTRA.clap $out/lib/vst3/KONTRA.vst3/Contents/x86_64-linux/KONTRA.so; do
            patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} "$f"
          done
        '';

        meta = {
          description = "KONTRA sampler plugin (CLAP and VST3)";
          license = lib.licenses.asl20;
          platforms = [ "x86_64-linux" ];
        };
      };
      # A bare CLAP host that opens the editor in an X11 window and
      # screenshots it: the UI check that needs no DAW (tools/clap-probe).
      clap-probe = pkgs.rustPlatform.buildRustPackage {
        pname = "clap-probe";
        version = "0.0.0";
        inherit (kontra) src;
        cargoLock.lockFile = ./Cargo.lock;
        cargoBuildFlags = [ "-p" "clap-probe" ];
        doCheck = false;
      };

      # The same check for VST3 (tools/vst3-probe): a bare host that serves
      # IPlugFrame and IRunLoop and attaches the editor to an X11 window.
      vst3-probe = pkgs.stdenv.mkDerivation {
        pname = "vst3-probe";
        version = "0.0.0";
        src = ./tools/vst3-probe;
        buildInputs = with pkgs; [ libx11 libpng ];
        buildPhase = "$CXX -std=c++17 -O2 -Wall -Wextra -o vst3-probe vst3_probe.cpp -lX11 -lpng -ldl";
        installPhase = "install -Dm755 vst3-probe $out/bin/vst3-probe";
      };
    in
    {
      packages.${system} = {
        default = kontra;
        inherit clap-probe vst3-probe;
      };

      # `nix flake check`: each format's editor opens in a bare host under
      # Xvfb, with Mesa's software Vulkan as the only driver, and must
      # render through the GPU path (a CPU fallback means the plugin could
      # not reach the graphics libraries) and draw. The screenshots land in
      # the check's output.
      checks.${system} =
        let
          uiCheck = name: probe: plugin: pkgs.runCommand "kontra-ui-${name}" {
            nativeBuildInputs = [ pkgs.xvfb-run ];
          } ''
            export HOME=$TMPDIR
            export VK_DRIVER_FILES=${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json
            export __EGL_VENDOR_LIBRARY_FILENAMES=${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json
            mkdir -p $out
            xvfb-run -s "-screen 0 1920x1200x24" \
              ${probe}/bin/${probe.pname} ${plugin} $out/editor.png 6 2>&1 | tee $out/log
            grep -q "GPU ready backend=Vulkan" $out/log
            grep -q "closed cleanly" $out/log
          '';
        in
        {
          ui-clap = uiCheck "clap" clap-probe "${kontra}/lib/clap/KONTRA.clap";
          ui-vst3 = uiCheck "vst3" vst3-probe "${kontra}/lib/vst3/KONTRA.vst3";
        };

      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ kontra ];
        packages = with pkgs; [ rustc cargo clippy rustfmt rust-analyzer xvfb-run mesa ];
        LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
        # Mesa's software Vulkan, for GPU tests on a machine without a GPU
        # (CI): `VK_DRIVER_FILES=$KONTRA_SOFTWARE_VULKAN cargo test ...`.
        KONTRA_SOFTWARE_VULKAN = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";
      };
    };
}
