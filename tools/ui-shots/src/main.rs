//! Draws each instrument's Original view headless, the way the editor will,
//! and says what it cost: first frame, settled, paint, idle, RAM. Saves the
//! settled page as `<out>/<name>-<slot>.png`, and the face before any control
//! moves as `<out>/<name>-<slot>-initial.png`. `UI_SHOTS_SCALE` sets the zoom.
//! With `--app`, the page is also drawn where the user meets it, inside an
//! open part of the rack, and saved as `<out>/<name>-<slot>-app.png`.
//! `cargo run --release -p ui-shots -- [--press <control>] [--app] <out dir> <instrument>...`

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use contra_engine::Engine;
use contra_host::ui::{Library, Live};
use contra_stream::{Store, StoreConfig};
use contra_ui::{Pictures, view};
use contra_ui_model::{Kind, PageRef, WidgetRef};
use mui::prelude::{Input, Size, Ui};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    // `--press <control>`: press that control before the pedal, so a page
    // the source shows on a press is measured too.
    let press = args.iter().position(|a| a == "--press").map(|i| {
        let name = args.get(i + 1).map(|n| n.to_string_lossy().into_owned());
        args.drain(i..(i + 2).min(args.len()));
        name.unwrap_or_default()
    });
    let app = args
        .iter()
        .position(|a| a == "--app")
        .map(|i| args.remove(i));
    let mut args = args.into_iter();
    let out = PathBuf::from(
        args.next()
            .ok_or("usage: ui-shots [--press <control>] <out dir> <instrument>...")?,
    );
    std::fs::create_dir_all(&out)?;
    for path in args.map(PathBuf::from) {
        let t = Instant::now();
        let mut store = Store::new(StoreConfig::DEFAULT)?;
        let loaded = contra_host::instrument::load(&path, 48_000, &mut store)?;
        let mut engine = Engine::new(Arc::new(loaded.plan), store.client(), 48_000);
        let load = t.elapsed();
        let mut block = vec![[0f32; 2]; 512];
        for (slot, face) in loaded.faces.iter().enumerate() {
            let Some(face) = face else { continue };
            let slot = slot as u8;
            let t = Instant::now();
            let library = RefCell::new(Library::open(&path));
            let meta = |p: &str| library.borrow_mut().meta(p);
            let mut live = Live::new(face, slot, &engine, &meta)?;
            let all = 0..live.tree.widgets.len();
            contra_ui::layout::resolve(&mut live.tree, all);
            // What a settled editor shows: listeners run in the first blocks.
            for _ in 0..10 {
                engine.process(&mut block, &[]);
                let changed = live.refresh(face, slot, &engine, &meta);
                contra_ui::layout::resolve(&mut live.tree, changed.widgets.iter().map(|w| w.0));
            }
            let tree_ms = t.elapsed();
            let Some(p) = live.tree.pages.first() else {
                continue;
            };
            let scale = scale();
            let (w, h) = (
                (p.size.width * scale).ceil() as u16,
                (p.size.height * scale).ceil() as u16,
            );
            let start = Instant::now();
            // The worker opens the library itself: the first frame waits on nothing.
            let (reader, library) = (path.clone(), std::sync::Mutex::new(None));
            let mut pictures = Pictures::new(Box::new(move |p: &str| {
                let mut library = library.lock().ok()?;
                library
                    .get_or_insert_with(|| Library::open(&reader))
                    .read(p)
            }));
            let mut ui = contra_ui::ui();
            let frame = |ui: &mut Ui, pictures: &mut Pictures, live: &Live| {
                let (el, built) = view::page(ui, 0, &live.tree, PageRef(0), pictures, (scale, &[]));
                ui.frame(
                    el,
                    Some(Size::new(f64::from(w), f64::from(h))),
                    Input::default(),
                    1. / 60.,
                )
                .map(|_| built)
                .map_err(|e| format!("{e:?}"))
            };
            frame(&mut ui, &mut pictures, &live)?;
            let first = (start.elapsed(), pictures.pending());
            while pictures.pending() > 0 {
                if pictures.poll() {
                    frame(&mut ui, &mut pictures, &live)?;
                } else {
                    std::thread::sleep(Duration::from_micros(500));
                }
            }
            frame(&mut ui, &mut pictures, &live)?;
            let settled = start.elapsed();
            // An idle editor tick: the engine runs, the view asks what
            // changed and frames only when something did.
            let (mut idle, mut idle_built, mut idle_frames) = (Vec::new(), 0, 0);
            for _ in 0..60 {
                engine.process(&mut block, &[]);
                let t = Instant::now();
                let changed = live.refresh(face, slot, &engine, &meta);
                if pictures.poll() || !changed.is_empty() {
                    contra_ui::layout::resolve(&mut live.tree, changed.widgets.iter().map(|w| w.0));
                    idle_built += frame(&mut ui, &mut pictures, &live)?;
                    idle_frames += 1;
                }
                idle.push(t.elapsed());
            }
            idle.sort();
            // The face as it loads, for the rack shot: the timing moves below
            // change what a library shows (ANALOG's knob shows its value).
            let untouched = live.tree.clone();
            // The face before anything moves: what the oracle's screenshot shows.
            let stem = loaded.name.replace(['/', ' '], "_");
            shot(
                &untouched,
                &path,
                &out.join(format!("{stem}-{slot}-initial.png")),
            )?;
            controls(
                &untouched,
                &out.join(format!("{stem}-{slot}-controls.jsonl")),
            )?;
            // One shown knob moved: the widgets that frame builds.
            let knob = live.tree.widgets.iter().enumerate().find_map(|(n, w)| {
                let knob = matches!(w.kind, Kind::Knob { .. } | Kind::Slider { .. });
                (knob && live.tree.visible(WidgetRef(n)))
                    .then_some(w.source_id)
                    .flatten()
            });
            let mut moved = None;
            if let Some(id) = knob {
                engine.set_control(slot, id, 1);
                engine.process(&mut block, &[]);
                let changed = live.refresh(face, slot, &engine, &meta);
                contra_ui::layout::resolve(&mut live.tree, changed.widgets.iter().map(|w| w.0));
                let mut built = frame(&mut ui, &mut pictures, &live)?;
                while pictures.pending() > 0 {
                    pictures.poll();
                    std::thread::sleep(Duration::from_micros(500));
                }
                built += frame(&mut ui, &mut pictures, &live)?;
                moved = Some(built);
            }
            let pressed = press
                .as_deref()
                .and_then(|p| live.tree.widgets.iter().find(|w| w.name == p)?.source_id);
            if let Some(id) = pressed {
                engine.set_control(slot, id, 1);
                for _ in 0..10 {
                    engine.process(&mut block, &[]);
                    let changed = live.refresh(face, slot, &engine, &meta);
                    contra_ui::layout::resolve(&mut live.tree, changed.widgets.iter().map(|w| w.0));
                }
                frame(&mut ui, &mut pictures, &live)?;
                while pictures.pending() > 0 {
                    pictures.poll();
                    std::thread::sleep(Duration::from_micros(500));
                }
                frame(&mut ui, &mut pictures, &live)?;
            }
            // The sustain pedal down, then up: what a controller callback
            // lights (Una Corda's pedal light), and what that repaints.
            let mut pedal = [0; 2];
            for (built, v) in pedal.iter_mut().zip([127, 0]) {
                let cc = contra_engine::Timed {
                    at: contra_engine::SampleTime(0),
                    body: contra_engine::Event::midi1(0xB0, 64, v),
                };
                engine.process(&mut block, &[cc]);
                for _ in 0..50 {
                    engine.process(&mut block, &[]);
                    let changed = live.refresh(face, slot, &engine, &meta);
                    if pictures.poll() || !changed.is_empty() {
                        contra_ui::layout::resolve(
                            &mut live.tree,
                            changed.widgets.iter().map(|w| w.0),
                        );
                        *built += frame(&mut ui, &mut pictures, &live)?;
                    }
                }
                while pictures.pending() > 0 {
                    pictures.poll();
                    std::thread::sleep(Duration::from_micros(500));
                }
                *built += frame(&mut ui, &mut pictures, &live)?;
            }
            let t = Instant::now();
            let rgba = paint(&ui, w, h)?;
            let paint_ms = t.elapsed();
            let file = out.join(format!(
                "{}-{slot}.png",
                loaded.name.replace(['/', ' '], "_")
            ));
            save(&file, w, h, &rgba)?;
            let drawn = (0..live.tree.widgets.len())
                .filter(|&n| live.tree.visible(contra_ui_model::WidgetRef(n)))
                .count();
            let knob_built = moved.map_or("null".into(), |m: usize| m.to_string());
            println!(
                "{{\"name\":{:?},\"slot\":{slot},\"widgets\":{},\"shown\":{drawn},\"pictures\":{},\"load_ms\":{:.0},\"tree_ms\":{:.1},\"first_frame_ms\":{:.2},\"first_pending\":{},\"settled_ms\":{:.1},\"paint_ms\":{:.1},\"idle_us\":{},\"idle_built\":{idle_built},\"idle_frames\":{idle_frames},\"knob_built\":{knob_built},\"pedal_built\":{pedal:?},\"images_mb\":{:.1},\"rss_mb\":{},\"peak_rss_mb\":{},\"shot\":{:?}}}",
                loaded.name,
                live.tree.widgets.len(),
                live.tree.assets.len(),
                ms(load),
                ms(tree_ms),
                ms(first.0),
                first.1,
                ms(settled),
                ms(paint_ms),
                idle.get(idle.len() / 2).map_or(0, Duration::as_micros),
                pictures.bytes() as f64 / 1e6,
                status("VmRSS:"),
                status("VmHWM:"),
                file,
            );
            if app.is_some() {
                let name = loaded.name.replace(['/', ' '], "_");
                let file = out.join(format!("{name}-{slot}-app.png"));
                let tree = if press.is_some() {
                    live.tree.clone()
                } else {
                    untouched
                };
                in_rack(&loaded.name, tree, pictures, &file)?;
            }
        }
    }
    Ok(())
}

/// The page inside an open part of the editor, 1180×900, saved to `file`
/// once its pictures are in.
fn in_rack(
    name: &str,
    tree: contra_ui_model::Interface,
    pictures: Pictures,
    file: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use contra_ui::app::{self, App, Face, Part, PartTab};
    let mut app = App {
        parts: vec![Part {
            name: name.to_owned(),
            library: name.to_owned(),
            channel: 1,
            output: 1,
            keys: (21, 108),
            face: Some(Face {
                tree,
                pictures,
                gesture: contra_ui::input::Gesture::default(),
                meters: Default::default(),
            }),
            open: true,
            tab: PartTab::Interface,
            ..Part::default()
        }],
        width: 1180.,
        ..App::default()
    };
    let (w, h) = (1180u16, 900u16);
    let mut ui = contra_ui::ui();
    let frame = |ui: &mut Ui, app: &mut App| -> Result<bool, Box<dyn std::error::Error>> {
        let r = app::view(ui, app);
        let f = ui
            .frame(
                r.el,
                Some(Size::new(f64::from(w), f64::from(h))),
                Input::default(),
                1. / 60.,
            )
            .map_err(|e| format!("{e:?}"))?;
        Ok(f.animating)
    };
    let pending = |app: &mut App| {
        app.parts
            .first_mut()
            .and_then(|p| p.face.as_mut())
            .map_or(0, |f| {
                f.pictures.poll();
                f.pictures.pending()
            })
    };
    for _ in 0..600 {
        let moving = frame(&mut ui, &mut app)?;
        if !moving && pending(&mut app) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    frame(&mut ui, &mut app)?;
    save(file, w, h, &paint(&ui, w, h)?)
}

/// Draws page 0 of `tree` once its pictures are in, and saves it.
fn shot(
    tree: &contra_ui_model::Interface,
    path: &std::path::Path,
    file: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(p) = tree.pages.first() else {
        return Ok(());
    };
    let scale = scale();
    let (w, h) = (
        (p.size.width * scale).ceil() as u16,
        (p.size.height * scale).ceil() as u16,
    );
    let (reader, library) = (path.to_path_buf(), std::sync::Mutex::new(None));
    let mut pictures = Pictures::new(Box::new(move |p: &str| {
        let mut library = library.lock().ok()?;
        library
            .get_or_insert_with(|| Library::open(&reader))
            .read(p)
    }));
    let mut ui = contra_ui::ui();
    let frame = |ui: &mut Ui, pictures: &mut Pictures| {
        let (el, _) = view::page(ui, 0, tree, PageRef(0), pictures, (scale, &[]));
        ui.frame(
            el,
            Some(Size::new(f64::from(w), f64::from(h))),
            Input::default(),
            1. / 60.,
        )
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
    };
    // A frame can ask for more pictures (stock art, then the fonts it
    // shows): frame again until nothing is pending.
    frame(&mut ui, &mut pictures)?;
    while pictures.pending() > 0 {
        while pictures.pending() > 0 {
            if !pictures.poll() {
                std::thread::sleep(Duration::from_micros(500));
            }
        }
        frame(&mut ui, &mut pictures)?;
    }
    save(file, w, h, &paint(&ui, w, h)?)
}

/// The page size, then every shown control a player can move, one JSON line
/// each: its widget
/// number, name, kind and page rectangle in points, for the real-window
/// sweep (`tools/contra-window/sweep.py`).
fn controls(
    tree: &contra_ui_model::Interface,
    file: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;
    let mut lines = String::new();
    if let Some(p) = tree.pages.first() {
        writeln!(lines, "{{\"page\":[{},{}]}}", p.size.width, p.size.height)?;
    }
    for (n, w) in tree.widgets.iter().enumerate() {
        // A range with one value (Pacific's `ui_value_edit $x(-180,-180,1)`)
        // cannot move in Kontakt either: not a control to sweep.
        let kind = match w.kind {
            Kind::Knob { range, .. }
            | Kind::Slider { range, .. }
            | Kind::ValueEdit { range, .. }
                if range.min.total_cmp(&range.max).is_eq() =>
            {
                continue;
            }
            Kind::Knob { .. } => "knob",
            Kind::Slider { .. } => "slider",
            Kind::Button { .. } => "button",
            Kind::Switch => "switch",
            Kind::Menu { .. } => "menu",
            Kind::ValueEdit { .. } => "value_edit",
            Kind::Table { .. } => "table",
            Kind::Xy { .. } => "xy",
            _ => continue,
        };
        if !tree.visible(WidgetRef(n)) || !w.enabled {
            continue;
        }
        let r = tree.page_rect(WidgetRef(n));
        writeln!(
            lines,
            "{{\"widget\":{n},\"name\":{:?},\"kind\":\"{kind}\",\"rect\":[{},{},{},{}]}}",
            w.name, r.x, r.y, r.width, r.height
        )?;
    }
    std::fs::write(file, lines)?;
    Ok(())
}

/// `UI_SHOTS_SCALE=<f>` draws pages at that zoom (1.5 matches the oracle's
/// 150% Kontakt screenshots); 1 otherwise.
fn scale() -> f64 {
    std::env::var("UI_SHOTS_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|s: &f64| *s > 0.)
        .unwrap_or(1.)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}

/// A `/proc/self/status` size line, in MB.
fn status(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            let line = s.lines().find(|l| l.starts_with(key))?.to_owned();
            line.split_whitespace().nth(1)?.parse::<u64>().ok()
        })
        .map_or(0, |kb| kb / 1024)
}

fn paint(ui: &Ui, w: u16, h: u16) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let scene = ui.scene().ok_or("no scene")?;
    Ok(mui_headless::Raster::new(w, h).draw(scene, 1.0)?)
}

fn save(
    file: &std::path::Path,
    w: u16,
    h: u16,
    rgba: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(mui_headless::write_png(file, w.into(), h.into(), rgba)?)
}
