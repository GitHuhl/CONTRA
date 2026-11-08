# CONTRA

A free, open-source sampler plugin that plays Kontakt libraries on NixOS.

- **Kontakt only.** Instruments, multis, snapshots and monoliths, with their
  KSP scripts, effects and performance views.
- **NixOS only.** CLAP and VST3, built entirely through Nix, graphics
  toolchain included.
- **Offline.** No telemetry, crash reporting, update checks or accounts.
  Nothing in it touches the network.

Status: alpha. Expect gaps and changes to saved state between versions.

## Build

From the repository folder:

```bash
nix develop
```

That drops you into a shell with everything needed. Then one command:

| Command | Does |
| :--- | :--- |
| `make clap` | Builds `build/CONTRA.clap` |
| `make vst3` | Builds `build/CONTRA.vst3` |
| `make` | Builds both |
| `make install` | Builds both and copies them to `~/.clap` and `~/.vst3`, where DAWs look |
| `make test` | Runs the tests |
| `make clean` | Deletes build output |

The first build takes several minutes; later ones are quicker.

## Install on NixOS (from your system config)

Instead of `make install`, you can install it like any other package. Add the
flake to your system flake:

```nix
inputs.contra.url = "github:GitHuhl/CONTRA";
```

Then add the package, system-wide or with Home Manager:

```nix
environment.systemPackages = [ inputs.contra.packages.x86_64-linux.default ];
```

```nix
home.packages = [ inputs.contra.packages.x86_64-linux.default ];
```

The plugins then sit in `/run/current-system/sw/lib/clap` and `.../lib/vst3`
(or `~/.nix-profile/lib/clap` and `.../lib/vst3` with Home Manager). Add those
folders to your DAW's plugin paths if it does not search them already.

## Why the editor opens on NixOS

The editor draws with Vulkan (falling back to the CPU) and loads Vulkan, EGL,
X11 and Wayland at run time. A DAW on NixOS has none of those on a library
search path, so the package bakes their store paths into the plugin itself:
it opens in any host without `LD_LIBRARY_PATH` or wrappers, and finds your
GPU driver where NixOS puts it (`/run/opengl-driver`).

## Checks

```bash
nix flake check
```

Builds the plugin, then opens its CLAP and VST3 editors in two bare test hosts
(`tools/clap-probe`, `tools/vst3-probe`) on a virtual screen with software
Vulkan. It fails unless the editor draws through the GPU path, and keeps a
screenshot of each. CI runs the same on every push to `main`.

## Layout

| Path | What |
| :--- | :--- |
| `crates/format/kontakt` | Reads Kontakt files into the instrument model |
| `crates/script/ksp` | KSP compiler |
| `crates/model` | Instrument model, parameters, events, UI model |
| `crates/engine` | Voices, streaming, effects, script VM |
| `crates/dsp` | Filters, EQ, dynamics, delay, reverb, drive, modulators |
| `crates/app` | Rack, persistence, library browser, editor UI |
| `plugin` | The CLAP/VST3 plugin |
| `vendor` | Vendored dependencies, including the plugin framework and UI toolkit |
| `docs/re` | Kontakt reverse-engineering evidence the engine's laws cite |
| `tools` | Inspector, screenshot tools, probe hosts, Kontakt reference scripts |

## License

Apache-2.0: see `LICENSE`, `NOTICE` and `THIRD_PARTY.md`.

CONTRA is an independent project. It began as a fork of DerpcatMusic/KONTRA
and is not affiliated with it. It is also not affiliated with or endorsed by
Native Instruments; Kontakt is their trademark.
