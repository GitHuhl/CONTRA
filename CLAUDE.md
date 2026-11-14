# CONTRA

A Kontakt-compatible sampler plugin (CLAP, VST3) for NixOS. Read
[README.md](README.md) first.

Scope, without exceptions:

- **Kontakt only.** No other sampler formats or script languages, with one
  carve-out: the Luau in `crates/script/lua` runs Kontakt 8's `native_ui`
  pages, nothing else (no UVI scripting).
- **NixOS only.** Linux x86_64; builds go through the flake (`nix build`,
  `nix flake check`, `nix develop`). No Windows/macOS code, targets or
  packaging.
- **No network.** No telemetry, crash reporting, update checks or any other
  outbound connection in the app or its dependencies.
- **One branch** (`main`). The build fetches nothing from git: dependencies
  that are not on crates.io live in `vendor/`.

Catching up with KONTRA (the project CONTRA forked from) goes through
`tools/upstream` (see its README). Nothing is sent to KONTRA unless the
user asks; changes worth offering it are listed in
`docs/UPSTREAM_CANDIDATES.md`.

Before claiming an editor/UI change works, run `nix flake check`: it opens
the CLAP and VST3 editors in bare hosts and keeps the screenshots.
