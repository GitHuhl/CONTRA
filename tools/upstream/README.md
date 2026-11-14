# Catching up with KONTRA

CONTRA is an independent fork of DerpcatMusic/KONTRA. To take his later
work without his scope (other formats, UVI Lua, website, release CI,
telemetry), both ends of his change are first put into CONTRA's terms,
then the difference is replayed here.

**Last caught up to: KONTRA `01bfb8b598258e55d3fddd92cc53816a60050e1a`.**
Use that as `OLD` next time, and update this line afterwards.

## Steps

```bash
git remote add upstream https://github.com/DerpcatMusic/KONTRA.git   # once
git fetch upstream main
```

1. Make two scratch commits, KONTRA's old and new trees, each passed
   through the scope tool, the new one on top of the old:

   ```bash
   git worktree add -b xform-base ../xf OLD
   cargo run -p upstream-scope -- ../xf
   git -C ../xf add -A && git -C ../xf commit -qm "KONTRA OLD in CONTRA scope"
   git -C ../xf checkout -qb xform-main
   git -C ../xf rm -rq . && git archive NEW | tar -x -C ../xf
   cargo run -p upstream-scope -- ../xf
   git -C ../xf add -A && git -C ../xf commit -qm "KONTRA NEW in CONTRA scope"
   ```

2. On a branch off `main`, replay that one commit as a three-way merge:

   ```bash
   git cherry-pick xform-main
   ```

   Afterwards remove the scratch: `git worktree remove --force ../xf`,
   `git branch -D xform-base xform-main`.

3. Resolve conflicts, then let the compiler find what his new code brings
   back that CONTRA does not carry (UVI/SFZ paths, Lua widgets, Windows or
   macOS branches, anything that reaches the network). `cargo test
   --workspace` and `nix flake check` must pass.

4. The frameworks (`vendor/moose*`, `vendor/mui*`, `vello_cpu`, `xim-rs`)
   and `Cargo.lock` are left out of the transform: when KONTRA moves to new
   revisions of them, vendor those revisions by hand, keep only Linux
   targets, drop reporting/telemetry features, and re-apply CONTRA's X11
   fixes in `vendor/moose-baseview` (see `docs/UPSTREAM_CANDIDATES.md`).

## What the scope tool does

`src/main.rs` lists it: it deletes what CONTRA does not carry, renames
kontra to contra, and keeps only Kontakt test data (two `.nki`, the KSP
conformance cases) from his tools folder.
