//! The editor's "Add folder": the desktop portal's folder picker
//! (`mui-native-dialog`), off the editor's thread, so no frame waits on it.
//! [`crate::Tick`] takes what was picked and scans it.
//!
//! Dropping [`Dialog`] at plugin teardown cancels an open picker and joins
//! its worker, so no thread is left running in an unloaded binary.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::lock;

const TITLE: &str = "A folder of libraries";

/// The "Add folder" picker while it is open.
#[derive(Default)]
pub struct Dialog {
    service: mui_native_dialog::DialogService,
    job: Mutex<Option<mui_native_dialog::DialogJob>>,
}

impl Dialog {
    /// Opens the picker at `from`, unless one is open.
    pub fn ask(&self, from: Option<PathBuf>) {
        let mut job = lock(&self.job);
        if job.is_some() {
            return;
        }
        let from = from.or_else(dirs::home_dir);
        *job = self.open(from);
    }

    fn open(&self, from: Option<PathBuf>) -> Option<mui_native_dialog::DialogJob> {
        use mui_native_dialog::{DialogKind, DialogRequest};
        let request = DialogRequest {
            parent: None,
            title: TITLE.into(),
            kind: DialogKind::PickFolder { multiple: false },
            directory: from,
            filters: Vec::new(),
        };
        self.service.spawn(request).ok()
    }

    /// The folder picked, once the picker closed.
    pub fn picked(&self) -> Option<PathBuf> {
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
