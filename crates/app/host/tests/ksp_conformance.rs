//! KSP conformance against Kontakt 8: each case's init-time statements run in
//! our engine, and every result must equal what Kontakt itself computed
//! (`ksp_conformance_cases.rs`, measured on Kontakt 8).

use std::sync::Arc;

use contra_engine::{Engine, Plan};
use contra_ksp::Cells;
use contra_stream::{Store, StoreConfig};

mod ksp_conformance_cases;
use ksp_conformance_cases::CASES;

/// The results of one case as our engine computes them.
fn run(setup: &str, results: &[&str]) -> Result<Vec<i32>, String> {
    let mut source = format!("on init\n  declare %r[{}]\n", results.len());
    for line in setup.lines() {
        source.push_str(&format!("  {line}\n"));
    }
    for (k, expr) in results.iter().enumerate() {
        source.push_str(&format!("  %r[{k}] := {expr}\n"));
    }
    source.push_str("end on\n");
    let script = contra_ksp::compile(&source, &[]).map_err(|e| format!("compile: {}", e.message))?;
    let base = script
        .variables
        .iter()
        .find(|v| &*v.name == "%r")
        .and_then(|v| match v.cells {
            Cells::Int { base, .. } => Some(base),
            _ => None,
        })
        .ok_or("no %r")?;
    let store = Store::new(StoreConfig::DEFAULT).map_err(|e| e.to_string())?;
    let mut plan = Plan::new(Vec::new(), 16);
    plan.scripts = vec![contra_host::script::plan(&script, &[], &[])];
    let mut engine = Engine::new(Arc::new(plan), store.client(), 48_000);
    engine.process(&mut [[0.0; 2]; 64], &[]);
    (0..results.len())
        .map(|k| {
            let cell = engine.script_cell_in(0, base as usize + k).ok_or("no cell")?;
            Ok(cell as i32)
        })
        .collect()
}

#[test]
fn init_time_results_match_kontakt() {
    let mut failures = Vec::new();
    for &(id, setup, results, kontakt) in CASES {
        match run(setup, results) {
            Ok(ours) if ours == kontakt => {}
            Ok(ours) => failures.push(format!("{id}: Kontakt {kontakt:?}, ours {ours:?}")),
            Err(e) => failures.push(format!("{id}: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from Kontakt:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}
