//! The editor's "Add folder": the desktop portal's folder picker
//! (`mui-native-dialog`), off the editor's thread, so no frame waits on it.
//! [`crate::Tick`] takes what was picked and scans it.
//!
//! Dropping [`Dialog`] at plugin teardown cancels an open picker and joins
//! its worker, so no thread is left running in an unloaded binary.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::lock;

/// What a picked folder is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wanted {
    /// One more folder of libraries to scan.
    #[default]
    Libraries,
    /// Kontakt's user snapshot folder ([`contra_library::Prefs::kontakt_snapshots`]).
    Snapshots,
}

impl Wanted {
    fn title(self) -> &'static str {
        match self {
            Self::Libraries => "A folder of libraries",
            Self::Snapshots => "Kontakt's user snapshot folder (User Content)",
        }
    }
}

/// The "Add folder" picker while it is open.
#[derive(Default)]
pub struct Dialog {
    service: mui_native_dialog::DialogService,
    job: Mutex<Option<mui_native_dialog::DialogJob>>,
    /// What the open picker's folder is for.
    wanted: Mutex<Wanted>,
}

impl Dialog {
    /// Opens the picker at `from` for a folder `wanted` for, unless one is
    /// open.
    pub fn ask(&self, wanted: Wanted, from: Option<PathBuf>) {
        let mut job = lock(&self.job);
        if job.is_some() {
            return;
        }
        *lock(&self.wanted) = wanted;
        let from = from.or_else(dirs::home_dir);
        *job = self.open(wanted.title(), from);
    }

    fn open(&self, title: &str, from: Option<PathBuf>) -> Option<mui_native_dialog::DialogJob> {
        use mui_native_dialog::{DialogKind, DialogRequest};
        let request = DialogRequest {
            parent: None,
            title: title.into(),
            kind: DialogKind::PickFolder { multiple: false },
            directory: from,
            filters: Vec::new(),
        };
        self.service.spawn(request).ok()
    }

    /// The folder picked and what for, once the picker closed.
    pub fn picked(&self) -> Option<(Wanted, PathBuf)> {
        let path = self.closed()?;
        Some((*lock(&self.wanted), path))
    }

    /// The folder picked, once the picker closed.
    fn closed(&self) -> Option<PathBuf> {
        let mut job = lock(&self.job);
        // Finished first: the worker sends its answer before it finishes.
        let finished = job.as_ref()?.is_finished();
        let result = job.as_ref()?.try_result();
        if result.is_none() && !finished {
            return None;
        }
        job.take();
        result?.ok()??.into_iter().next()
    }
}
