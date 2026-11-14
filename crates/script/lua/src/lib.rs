//! Luau for Kontakt 8 `native_ui` pages ([`nui`]): a library's page modules,
//! rewritten to Luau, run in a sandboxed state and rendered to a tree of
//! primitive nodes for the editor to lay out. Control thread only; nothing
//! here runs on the audio thread. CONTRA keeps only this part of KONTRA's
//! Lua crate: no other script language.
//!
//! The Luau heap lives in [`alloc`]'s arena: mlua 0.12 allocates through the
//! Rust global allocator, so this crate installs [`alloc::Routed`] as that
//! allocator. A binary linking this crate takes it; a second
//! `#[global_allocator]` fails to link.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use mlua::{Lua, LuaOptions, StdLib};

pub mod alloc;
mod interrupt;
pub mod nui;
pub mod probe;

// With `system-allocator` (the host's heap-profile example) the binary brings
// its own measuring allocator and Luau uses the system heap.
#[cfg(not(feature = "system-allocator"))]
#[global_allocator]
static ROUTED: alloc::Routed = alloc::Routed;

/// Luau heap per state.
const MEMORY: usize = 128 << 20;
/// Interrupt checkpoints (calls and loop back-edges) one run may take before
/// it is stopped.
const CHECKPOINTS: u32 = 10_000_000;
/// The first run (loading the page) is bounded separately.
const LOAD_CHECKPOINTS: u32 = 100_000_000;

/// A state with the standard libraries a page may use, its heap capped at
/// [`MEMORY`] and its runs counted: past the limit (at first
/// [`LOAD_CHECKPOINTS`]) a run stops. The count and the limit, to set.
fn sandbox() -> mlua::Result<(Lua, Arc<AtomicU32>, Arc<AtomicU32>)> {
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::COROUTINE | StdLib::BIT;
    let lua = Lua::new_with(libs, LuaOptions::new())?;
    lua.set_memory_limit(MEMORY)?;
    // Luau seeds `math.random` from the clock; a fixed seed draws the same
    // page on every load.
    lua.load("math.randomseed(0x5EED)").exec()?;
    // Installed once and bounding the load too: the caller zeroes the count
    // before each run, so a runaway loop stops the run, not the host.
    let checkpoints = Arc::new(AtomicU32::new(0));
    let count = Arc::clone(&checkpoints);
    let limit = Arc::new(AtomicU32::new(LOAD_CHECKPOINTS));
    let current_limit = Arc::clone(&limit);
    lua.set_interrupt(move |_| {
        if count.fetch_add(interrupt::STRIDE, Ordering::Relaxed)
            > current_limit.load(Ordering::Relaxed)
        {
            return Err(mlua::Error::runtime("script ran too long"));
        }
        Ok(mlua::VmState::Continue)
    });
    interrupt::install(&lua)?;
    Ok((lua, checkpoints, limit))
}
