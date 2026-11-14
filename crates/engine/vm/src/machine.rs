//! A script slot's machine: the KSP bytecode interpreter.

use crate::{CoId, Faults, HostApi, Instance, Program, Step, Text};

/// One script slot's machine.
pub struct Vm(Instance);

impl Vm {
    /// The interpreter with `coroutines` slots. Not real-time.
    #[must_use]
    pub fn new(program: &Program, coroutines: usize) -> Self {
        Self(Instance::new(program, coroutines.max(1)))
    }

    pub fn start(&mut self, entry: u32, context: u64) -> Option<CoId> {
        self.0.start(entry, context)
    }

    pub fn run(&mut self, program: &Program, co: CoId, host: &mut dyn HostApi) -> Step {
        self.0.run(program, co, host)
    }

    /// The next run yields after `turns` loop turns.
    pub fn set_fuel(&mut self, turns: u32) {
        self.0.set_fuel(turns);
    }

    /// Loop turns the last [`Vm::run`] made.
    #[must_use]
    pub fn last_turns(&self) -> u32 {
        self.0.last_turns()
    }

    pub fn cancel(&mut self, co: CoId) {
        self.0.cancel(co);
    }

    #[must_use]
    pub fn context(&self, co: CoId) -> Option<u64> {
        self.0.context(co)
    }

    #[must_use]
    pub fn wait_epoch(&self, co: CoId) -> Option<u32> {
        self.0.wait_epoch(co)
    }

    /// Whether a running (waiting) callback has a context `matches` takes.
    #[must_use]
    pub fn is_running(&self, matches: impl Fn(u64) -> bool) -> bool {
        self.0.find(matches).is_some()
    }

    /// `stop_wait` on the callback with this context; the epoch to resume it with.
    pub fn stop_wait(&mut self, matches: impl Fn(u64) -> bool, rest: bool) -> Option<(CoId, u32)> {
        let co = self.0.find(matches)?;
        Some((co, self.0.stop_wait(co, rest)?))
    }

    #[must_use]
    pub fn faults(&self) -> Faults {
        self.0.faults()
    }

    #[must_use]
    pub fn cell(&self, n: usize) -> Option<i64> {
        self.0.cell(n)
    }

    pub fn set_cell(&mut self, n: usize, value: i64) {
        self.0.set_cell(n, value);
    }

    pub fn set_text(&mut self, n: usize, value: Text) {
        self.0.set_text(n, value);
    }

    /// The interpreter's change count, wrapping: unchanged, its cells are as
    /// they were.
    #[must_use]
    pub fn runs(&self) -> u32 {
        self.0.changes
    }

    /// The interpreter's cells in `range`.
    #[must_use]
    pub fn cells(&self, range: std::ops::Range<usize>) -> Option<&[i64]> {
        self.0.cells.get(range)
    }

    /// The interpreter's text cells in `range`.
    #[must_use]
    pub fn texts(&self, range: std::ops::Range<usize>) -> Option<&[Text]> {
        self.0.texts.get(range)
    }

    #[must_use]
    pub fn text(&self, n: usize) -> Option<&Text> {
        self.0.text(n)
    }
}
