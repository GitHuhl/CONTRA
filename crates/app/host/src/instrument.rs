//! Loading an instrument file into a plan the engine plays: the format reads
//! it into the IR, prepare loads its samples, its script is compiled.

use std::path::Path;

use kontra_engine::{Plan, ir};
use kontra_prepare::prepare;
use kontra_stream::Store;

/// A plan ready for [`kontra_engine::Engine::new`], and what did not carry over.
pub struct Loaded {
    pub name: String,
    pub plan: Plan,
    pub diagnostics: Vec<ir::Diagnostic>,
    /// By script slot, what a KSP script's UI tree is built from
    /// ([`crate::ui::interface`] once the engine ran `on init`).
    pub faces: Vec<Option<crate::ui::Face>>,
    /// By script slot, its persistent variables ([`crate::persist`]).
    pub persist: Vec<crate::persist::Vars>,
}

/// Reads the Kontakt instrument at `path` into the model.
///
/// # Errors
/// When the file is not an instrument this can read.
pub fn read(path: &Path) -> Result<ir::Instrument, Box<dyn std::error::Error>> {
    Ok(kontra_format_kontakt::load(path)?)
}

/// Saved script values for an instrument held in another file, which
/// [`crate::rack::Rack::recall`] can apply to a part playing that file.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    /// The instrument file the values are for, absolute, as its IR's
    /// `source_path` names it.
    pub base: std::path::PathBuf,
    /// Saved values by script slot ([`ir::Script::slot`]); an empty slot
    /// means the base's own, which only a full load reads.
    pub saved: Vec<Vec<String>>,
}

/// The file at `path` as a [`Preset`] (a Kontakt snapshot), read without
/// its base instrument; None for anything else, or a snapshot this cannot
/// read (its full load then reports why).
#[must_use]
pub fn preset(path: &Path) -> Option<Preset> {
    let snapshot = (path.extension()).is_some_and(|e| e.eq_ignore_ascii_case("nksn"));
    let p = snapshot.then(|| kontra_format_kontakt::preset(path).ok())??;
    Some(Preset {
        name: p.name,
        base: p.base,
        saved: p.saved,
    })
}

/// Reads the instrument at `path` for an engine at `rate`.
///
/// # Errors
/// When the file is not an instrument this can read.
pub fn load(
    path: &Path,
    rate: u32,
    store: &mut Store,
) -> Result<Loaded, Box<dyn std::error::Error>> {
    Ok(build(read(path)?, rate, store))
}

/// Builds the plan for an instrument already read into the IR.
#[must_use]
pub fn build(instrument: ir::Instrument, rate: u32, store: &mut Store) -> Loaded {
    // Each file once, however many samples it holds.
    let files = kontra_stream::Files::new(instrument.samples.iter().map(|s| s.file.as_path()));
    let (mut plan, mut diagnostics) = prepare(&instrument, rate, store, &|s| files.open(s));
    let mut faces = Vec::new();
    let mut persist = Vec::new();
    diagnostics.extend(instrument.diagnostics);
    // ponytail: a slot that fails to compile is dropped, and the slots after
    // it move up one ($EVENT_PAR_SOURCE then counts from the gap).
    // Discovery walks the disk, so only a KSP script triggers it, and once.
    let mut factory_folder = None;
    for (n, script) in instrument.scripts.iter().enumerate() {
        match kontra_ksp::compile(&script.source, &[]) {
            Ok(compiled) => {
                let mut planned = crate::script::plan(&compiled, &script.saved, &script.data);
                planned.source_folder =
                    crate::script::source_folder(instrument.source_path.as_deref());
                planned.library_folder = crate::script::folder(instrument.library_path.as_deref());
                planned.files = Some(std::sync::Arc::new(crate::rack::DiskFiles::default()));
                planned.factory_folder = *factory_folder.get_or_insert_with(|| {
                    factory_folder_of(kontra_library::kontakt::candidates())
                });
                planned.links = crate::script::links(&compiled, &script.links);
                if planned.links.len() < script.links.len() {
                    diagnostics.push(ir::Diagnostic {
                        impact: ir::Impact::Audible,
                        feature: "MIDI automation of a missing script control".into(),
                        detail: script.name.clone(),
                    });
                }
                // Kontakt's own script plays release triggers and holds notes
                // under the sustain pedal unless a slot switches that part off.
                let off = |part: &str| {
                    compiled
                        .conditions
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(part))
                };
                if off("NO_SYS_SCRIPT_RLS_TRIG") {
                    plan.release_zones = kontra_engine::ReleaseZones::WithNote;
                }
                if off("NO_SYS_SCRIPT_PEDAL") {
                    plan.sustain_pedal = kontra_engine::SustainPedal::ToScripts;
                }
                plan.scripts.push(planned);
                faces.push(Some(crate::ui::Face::of(&compiled, &script.data)));
                persist.push(crate::persist::Vars::of(n, &compiled));
            }
            Err(e) => diagnostics.push(ir::Diagnostic {
                impact: ir::Impact::Broken,
                feature: "script compile".into(),
                detail: format!(
                    "{}: {} at `{}`",
                    script.name,
                    e.message,
                    line_at(&script.source, e.span.start as usize).trim()
                ),
            }),
        }
    }
    Loaded {
        name: instrument.name,
        plan,
        diagnostics,
        faces,
        persist,
    }
}

/// The source line holding byte `at`.
fn line_at(source: &str, at: usize) -> &str {
    let at = at.min(source.len());
    let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = source[at..].find('\n').map_or(source.len(), |i| at + i);
    source.get(start..end).unwrap_or_default()
}

/// The folder holding the first installed executable: Kontakt's factory data
/// folder, as `get_folder($GET_FOLDER_FACTORY_DIR)` reports it.
fn factory_folder_of(candidates: Vec<std::path::PathBuf>) -> Option<kontra_engine::Text> {
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .and_then(|path| crate::script::folder(path.parent()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn factory_folder_is_the_first_installed_executables_folder() {
        let dir = std::env::temp_dir().join(format!("kontra-factory-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Kontakt 8.exe");
        std::fs::write(&exe, b"").unwrap();
        let missing = dir.join("missing/Kontakt 7.exe");
        let found = super::factory_folder_of(vec![missing.clone(), exe]).unwrap();
        assert_eq!(
            found.as_str(),
            format!("{}/", dir.display()).replace('\\', "/")
        );
        assert!(super::factory_folder_of(vec![missing]).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
