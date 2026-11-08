//! A part's library interface on the control side: the tree the editor
//! shows, built once the part's `on init` ran, kept current with what the
//! script changes since ([`LookFrame`]), and how a widget the user moves
//! reaches the script that owns it. The tree is built from the KSP script's
//! controls.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use contra_engine::{ControlLook, Engine, FIRST_FONT, Key, MenuItem, Page, Text};
use contra_ui_model as model;

use crate::ui::{Changes, Face, Library, Live, Looks};

/// The interface of a part's first script with one.
pub struct View {
    pub slot: u8,
    live: Box<Live>,
    /// Moves whenever [`View::refresh`] changed the tree.
    pub revision: u64,
    /// The frame between trips to the audio side; `None` while it is there.
    pub(crate) frame: Option<Box<LookFrame>>,
    /// The library's resources, opened once and kept for later pictures.
    library: Option<Library>,
}

impl View {
    /// The view of the first script slot with a face, read from `engine`
    /// as its init left it; pictures come from the library at `path`.
    #[must_use]
    pub fn of(faces: &[Option<Face>], engine: &Engine, path: Option<&Path>) -> Option<Self> {
        let library = path.map(|p| RefCell::new(Library::open(p)));
        let meta = |p: &str| library.as_ref()?.borrow_mut().meta(p);
        let mut view = (0..faces.len()).find_map(|slot| {
            let at = u8::try_from(slot).ok()?;
            let face = faces.get(slot)?.as_ref()?;
            let live = Live::new(face, at, engine, &meta).ok()?;
            let frame = LookFrame::new(at, &live, face, engine);
            Some(Self::new(at, Box::new(live), frame))
        })?;
        view.library = library.map(RefCell::into_inner);
        Some(view)
    }

    fn new(slot: u8, live: Box<Live>, frame: Box<LookFrame>) -> Self {
        Self {
            slot,
            live,
            revision: 0,
            frame: Some(frame),
            library: None,
        }
    }

    /// The tree the editor shows.
    #[must_use]
    pub fn tree(&self) -> &model::Interface {
        &self.live.tree
    }

    /// Brings the tree up to `frame` (filled on the audio side), and keeps
    /// the frame for the next trip, grown if the script outgrew it.
    pub(crate) fn refresh(
        &mut self,
        faces: &[Option<Face>],
        path: Option<&Path>,
        frame: Box<LookFrame>,
    ) {
        let live = &mut self.live;
        let Some(face) = faces.get(usize::from(self.slot)).and_then(Option::as_ref) else {
            return;
        };
        let library = RefCell::new(self.library.take());
        // Opened only if a changed widget shows a picture, then kept.
        let meta = |p: &str| {
            let path = path?;
            let mut l = library.borrow_mut();
            l.get_or_insert_with(|| Library::open(path)).meta(p)
        };
        // An overflowed frame has stale lists under new generations: the
        // tree waits for the grown one rather than remember them as seen.
        if frame.state == Filled::Whole {
            let changes: Changes = live.refresh(face, self.slot, &*frame, &meta);
            if !changes.is_empty() {
                self.revision += 1;
            }
        }
        self.frame = Some(match frame.state {
            Filled::Overflowed => frame.grown(),
            Filled::No | Filled::Whole => frame,
        });
        self.library = library.into_inner();
    }

    /// The script control behind `widget`, as (slot, UI id), and `value`
    /// as the script's integer; None for a widget no control owns.
    #[must_use]
    pub fn control(&self, widget: usize, value: f64) -> Option<(u8, i32, i64)> {
        let w = self.tree().widgets.get(widget)?;
        let id = w.source_id?;
        let value = match w.value {
            Some(model::Value::Real(_)) => value.to_bits() as i64,
            _ => value.round() as i64,
        };
        Some((self.slot, id, value))
    }
}

/// One control's look as the audio side last copied it. Its lists keep
/// their room between trips: the audio side never allocates.
struct Look {
    id: i32,
    present: bool,
    value: Option<i64>,
    generation: u32,
    pars: Vec<(i32, i32)>,
    texts: Vec<(i32, Text)>,
    menu: Vec<MenuItem>,
}

/// What a part's [`Live`] reads of its script, copied out of the engine on
/// the audio thread ([`LookFrame::fill`]) for the control side to refresh
/// the tree from. Its ids and cells are sorted.
pub struct LookFrame {
    pub(crate) part: usize,
    pub(crate) generation: u64,
    slot: u8,
    looks: Box<[Look]>,
    cells: Box<[(u32, Option<i64>)]>,
    /// Text cells (`@` controls), copied in place.
    strings: Box<[(u32, Text)]>,
    page: Option<Page>,
    /// The script's bitmap fonts from [`FIRST_FONT`] on; `on init` names
    /// them all, so they are copied once.
    fonts: Arc<[String]>,
    /// The keyboard's keys, by note (a fixed 128: copied in place).
    keys: Vec<Key>,
    pressed_support: bool,
    state: Filled,
}

/// How a [`LookFrame`] came back from the audio side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filled {
    /// Not filled: new, or the part was gone or reloaded when it arrived.
    No,
    Whole,
    /// A list outgrew its room: some look is stale, grow and ask again.
    Overflowed,
}

/// A new frame's lists hold this many times what the script set so far,
/// and at least [`MIN_ROOM`]: more room, fewer trips that overflow and
/// grow, against more memory per control.
const ROOM: usize = 2;
const MIN_ROOM: usize = 4;

impl LookFrame {
    /// A frame for `live`'s reads, with [`ROOM`] times what `now` holds.
    fn new(slot: u8, live: &Live, face: &Face, now: &dyn Looks) -> Box<Self> {
        let (mut ids, mut cells, mut strings) = live.reads(face);
        strings.sort_unstable();
        strings.dedup();
        ids.sort_unstable();
        ids.dedup();
        cells.sort_unstable();
        cells.dedup();
        let looks = ids
            .into_iter()
            .map(|id| {
                let l = now.control_look(slot, id);
                let len = |n: usize| (n * ROOM).max(MIN_ROOM);
                Look {
                    id,
                    present: false,
                    value: None,
                    generation: 0,
                    pars: Vec::with_capacity(len(l.as_ref().map_or(0, |l| l.pars.len()))),
                    texts: Vec::with_capacity(len(l.as_ref().map_or(0, |l| l.texts.len()))),
                    menu: Vec::with_capacity(len(l.as_ref().map_or(0, |l| l.menu.len()))),
                }
            })
            .collect();
        Box::new(Self {
            part: 0,
            generation: 0,
            slot,
            looks,
            cells: cells.into_iter().map(|c| (c, None)).collect(),
            strings: strings.into_iter().map(|c| (c, Text::EMPTY)).collect(),
            page: None,
            fonts: (FIRST_FONT..)
                .map_while(|id| now.font(slot, id).map(str::to_owned))
                .collect(),
            keys: now.keys().to_vec(),
            pressed_support: now.pressed_support(),
            state: Filled::No,
        })
    }

    /// Copies the reads out of `engine`; on the audio thread, in place.
    pub(crate) fn fill(&mut self, engine: &Engine) {
        let mut whole = true;
        for l in &mut *self.looks {
            let Some(c) = engine.control_look(self.slot, l.id) else {
                l.present = false;
                continue;
            };
            l.value = c.value;
            // The lists move only with the look's generation.
            if !l.present || l.generation != c.generation {
                let ok = copy(&mut l.pars, c.pars)
                    & copy(&mut l.texts, c.texts)
                    & copy(&mut l.menu, c.menu);
                whole &= ok;
            }
            l.present = true;
            l.generation = c.generation;
        }
        for (cell, value) in &mut *self.cells {
            *value = engine.script_cell_in(self.slot, *cell as usize);
        }
        for (cell, text) in &mut *self.strings {
            let vm = engine.vm(self.slot);
            *text = vm
                .and_then(|vm| vm.text(*cell as usize))
                .copied()
                .unwrap_or(Text::EMPTY);
        }
        self.page = engine.page(self.slot).copied();
        // Same length every trip, so this never allocates.
        if !copy(&mut self.keys, engine.keys()) {
            whole = false;
        }
        self.pressed_support = engine.pressed_support();
        self.state = if whole {
            Filled::Whole
        } else {
            Filled::Overflowed
        };
    }

    /// The same reads with twice the room in every list, to ask again.
    fn grown(&self) -> Box<Self> {
        let looks = (self.looks.iter())
            .map(|l| Look {
                id: l.id,
                present: false,
                value: None,
                generation: 0,
                pars: Vec::with_capacity(l.pars.capacity() * 2),
                texts: Vec::with_capacity(l.texts.capacity() * 2),
                menu: Vec::with_capacity(l.menu.capacity() * 2),
            })
            .collect();
        Box::new(Self {
            looks,
            cells: self.cells.clone(),
            strings: self.strings.clone(),
            fonts: Arc::clone(&self.fonts),
            keys: self.keys.clone(),
            state: Filled::No,
            ..*self
        })
    }

    /// Not filled: the part was gone or reloaded when it arrived.
    pub(crate) fn unfilled(&mut self) {
        self.state = Filled::No;
    }
}

/// `to` as `from`, if it has room; else false and `to` is left as it was.
fn copy<T: Copy>(to: &mut Vec<T>, from: &[T]) -> bool {
    if from.len() > to.capacity() {
        return false;
    }
    to.clear();
    to.extend_from_slice(from);
    true
}

impl Looks for LookFrame {
    fn control_look(&self, slot: u8, id: i32) -> Option<ControlLook<'_>> {
        if slot != self.slot {
            return None;
        }
        let n = self.looks.binary_search_by_key(&id, |l| l.id).ok()?;
        let l = self.looks.get(n).filter(|l| l.present)?;
        Some(ControlLook {
            value: l.value,
            pars: &l.pars,
            texts: &l.texts,
            menu: &l.menu,
            generation: l.generation,
        })
    }

    fn script_cell_in(&self, slot: u8, n: usize) -> Option<i64> {
        let n = u32::try_from(n).ok().filter(|_| slot == self.slot)?;
        let at = self.cells.binary_search_by_key(&n, |c| c.0).ok()?;
        self.cells.get(at)?.1
    }

    fn script_text_in(&self, slot: u8, n: usize) -> Option<&str> {
        let n = u32::try_from(n).ok().filter(|_| slot == self.slot)?;
        let at = self.strings.binary_search_by_key(&n, |c| c.0).ok()?;
        Some(self.strings.get(at)?.1.as_str())
    }

    fn page(&self, slot: u8) -> Option<&Page> {
        self.page.as_ref().filter(|_| slot == self.slot)
    }

    fn font(&self, slot: u8, id: i32) -> Option<&str> {
        let n = usize::try_from(id.checked_sub(FIRST_FONT)?).ok()?;
        (self.fonts.get(n).filter(|_| slot == self.slot)).map(String::as_str)
    }

    fn keys(&self) -> &[Key] {
        &self.keys
    }

    fn pressed_support(&self) -> bool {
        self.pressed_support
    }
}
