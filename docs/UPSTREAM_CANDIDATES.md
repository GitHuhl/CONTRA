# Changes KONTRA could take

CONTRA fixes that would also help KONTRA (DerpcatMusic/KONTRA), checked
against KONTRA `01bfb8b59`. **None of these has been sent.** They go only
when the maintainer of this repository says so, one small PR each, on a
branch made from KONTRA's own `main`, without CONTRA's renames or scope
cuts.

Items 1 and 2 live in the windowing code KONTRA takes from Matari-Audio's
MUI repository (pinned by git in its `Cargo.toml`; CONTRA vendors MUI
`82ea0a9` as `vendor/moose-baseview`). They land there, or in KONTRA as a
`[patch]` until MUI takes them.

| # | Change | Where in CONTRA | Why KONTRA needs it |
| :--- | :--- | :--- | :--- |
| 1 | X11 viewability safety net | `vendor/moose-baseview/src/platform/x11/event_loop.rs` (`VIEWABLE_PROBE`, `server_says_viewable`), `visibility_tree.rs` (`mark_viewable`) | KONTRA took only the STRUCTURE_NOTIFY half of the earlier PR. When a host maps its frame without an event reaching the plugin (reparenting WMs, a DAW session reopened), the editor waits for its first frame forever. The probe asks the server every 200 ms while the window is alive but not known viewable. |
| 2 | X11 window open timeout, 250 ms to 5 s | `vendor/moose-baseview/src/platform/x11/window_thread.rs` (`OPEN_TIMEOUT`) | 250 ms is less than a loaded machine or a DAW restoring a session needs; the editor then fails to open and stays blank. |
| 3 | Graphics libraries on the plugin's RPATH | `flake.nix` (`runtimeLibs`, `dontPatchELF`, `postFixup`) | `x11-dl` dlopens `libX11-xcb.so.1` by name, which NixOS does not put on any search path: the "opening library failed (libX11-xcb.so.1)" blank window from the 0.6.5 report. The same holds for Vulkan, EGL, xkbcommon and Wayland. Patching their store paths into the `.so` lets it open in any host. |
| 4 | KSP decimal literals clamp | `crates/script/ksp/src/lex.rs` | Kontakt reads a decimal literal past 2147483647 as 2147483647 (measured); KONTRA keeps the full 64-bit value. Hex keeps its 32-bit pattern. |
| 5 | Bare CLAP/VST3 probe hosts and the Xvfb UI checks | `tools/clap-probe`, `tools/vst3-probe`, `flake.nix` `checks` | They open the editor the way DAWs do (embedded while hidden, then framed and mapped by a WM; reopened; reloaded) and fail when it never draws. They are what caught 1 and 2 without a DAW. |

Not candidates: the telemetry/reporting removal (KONTRA's choice), the
scope cuts and renames, Linux-only target pruning, and the Rust port of
tooling. These are CONTRA's direction, not fixes.
