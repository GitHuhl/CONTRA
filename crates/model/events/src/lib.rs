//! Event model: host notes, host patterns, note expression and the Universal
//! MIDI Packet event, plus the receiver wire value types. Plain data only; no
//! CONTRA crate is a dependency.

mod event;
mod wire;

pub use event::*;
pub use wire::*;
