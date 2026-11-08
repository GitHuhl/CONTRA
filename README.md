# KONTRA

A free, open-source sampler plugin for NixOS that plays Kontakt libraries.
CLAP and VST3. Nothing in it touches the network.

## Build

Everything builds through Nix, graphics toolchain included:

```bash
nix build
```

The plugin lands in `result/lib/clap/KONTRA.clap` and
`result/lib/vst3/KONTRA.vst3`. Point your DAW's plugin paths there, or install
the package with `environment.systemPackages` / `home.packages` and add
`/run/current-system/sw/lib/clap` (or `~/.nix-profile/lib/clap`) and the
matching `vst3` folder.

The editor loads Vulkan, EGL, X11 and Wayland at run time. The package bakes
their store paths into the plugin, so it opens in any host without
`LD_LIBRARY_PATH`, and it finds your GPU driver where NixOS puts it
(`/run/opengl-driver`). Without a usable GPU it falls back to CPU rendering.

## Check

```bash
nix flake check
```

`ui-clap` and `ui-vst3` open the editor in a bare host (`tools/clap-probe`,
`tools/vst3-probe`) under Xvfb with Mesa's software Vulkan, fail unless it
renders through the GPU path and draws, and keep the screenshot in their
output.

Unit and integration tests run in the dev shell:

```bash
nix develop
```

```bash
cargo test --workspace --exclude kontra-window
```

## Layout

| Path | What |
| :--- | :--- |
| `crates/format/kontakt` | Reads Kontakt instruments, multis, snapshots and monoliths into the IR |
| `crates/script/ksp` | KSP compiler |
| `crates/model` | The IR, parameters, events, UI model |
| `crates/engine` | Voices, streaming, effects, script VM: the one engine |
| `crates/dsp` | Filters, EQ, dynamics, delay, reverb, drive, modulators |
| `crates/app` | Host (rack, persistence, library browser) and editor UI |
| `plugin` | The CLAP/VST3 shell |
| `vendor` | Vendored dependencies, including the moose plugin framework and MUI |
| `docs/re` | Kontakt reverse-engineering evidence the laws cite |
| `tools` | Inspector, UI screenshots, probe hosts, Kontakt oracle scripts |

## License

Apache-2.0. See `LICENSE`, `NOTICE` and `THIRD_PARTY.md`.
