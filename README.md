# CONTRA

A free, open-source sampler plugin that plays Kontakt libraries on NixOS.

- **Kontakt only.** Instruments, multis, snapshots and monoliths, with their
  KSP scripts, effects and performance views.
- **NixOS only.** CLAP and VST3, built entirely through Nix, graphics
  toolchain included.
- **Offline.** No telemetry, crash reporting, update checks or accounts.
  Nothing in it touches the network.

Status: alpha. Expect gaps and changes to saved state between versions.

## Install on NixOS

Add the flake to your system flake:

```nix
inputs.contra.url = "github:GitHuhl/CONTRA";
```

Then install the package, system-wide or with Home Manager:

```nix
environment.systemPackages = [ inputs.contra.packages.x86_64-linux.default ];
```

```nix
home.packages = [ inputs.contra.packages.x86_64-linux.default ];
```

Your DAW finds the plugins in `/run/current-system/sw/lib/clap` and
`/run/current-system/sw/lib/vst3` (or `~/.nix-profile/lib/clap` and
`~/.nix-profile/lib/vst3` with Home Manager). Add those folders to its plugin
paths if it does not search them already.

To try it without installing:

```bash
nix build github:GitHuhl/CONTRA
```

The plugins land in `result/lib/clap/KONTRA.clap` and
`result/lib/vst3/KONTRA.vst3`.

## Why the editor opens on NixOS

The editor draws with Vulkan (falling back to the CPU) and loads Vulkan, EGL,
X11 and Wayland at run time. A DAW on NixOS has none of those on a library
search path, so the package bakes their store paths into the plugin itself:
it opens in any host without `LD_LIBRARY_PATH` or wrappers, and finds your
GPU driver where NixOS puts it (`/run/opengl-driver`).

## Develop

```bash
nix develop
```

```bash
cargo test --workspace
```

```bash
nix flake check
```

`nix flake check` builds the plugin, then opens its CLAP and VST3 editors in
two bare hosts (`tools/clap-probe`, `tools/vst3-probe`) under a virtual X
server with Mesa's software Vulkan. It fails unless the editor renders through
the GPU path and draws, and keeps a screenshot of each in its output. CI runs
the same on every push to `main`.

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
