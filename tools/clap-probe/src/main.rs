//! `clap-probe <plugin.clap> <out.png> [max seconds] [mode] [show ms] [flags…]`:
//! loads a CLAP plugin the way a DAW does (no environment help), embeds its
//! editor in an X11 window of the size it asks for, waits until it draws,
//! and writes what the window shows. Exits non-zero when the editor does not
//! open or draws nothing.
//!
//! Modes, how the host window appears:
//! - `normal`: one top-level, shown before embedding.
//! - `late`: the editor goes in a container inside a top-level that is still
//!   hidden; the host shows it after embedding.
//! - `wm`: as `late`, shown the way a reparenting window manager does: moved
//!   into a frame window, mapped, the frame mapped.
//! - `wm2`: as `wm`, with the editor's container hidden until then too.
//! - `reframe`: a top-level already showing is moved into a frame (X unmaps
//!   and remaps it).
//! - `flicker`: a top-level already showing is hidden and shown again.
//!
//! `show ms` is how long after embedding the host shows the window (default
//! 500); short delays race the editor's start-up.
//!
//! Flags, what else a DAW does:
//! - `audio`: activate and process audio on its own thread throughout.
//! - `reload`: as a DAW reopening a saved session: one instance saves its
//!   state; a fresh instance restores it (and starts audio, with `audio`)
//!   before its editor opens.
//! - `reopen`: once the editor has drawn, close it and open it again in a new
//!   window; both must draw.
//!
//! `CLAP_PROBE_SAVE=<file>` writes the state `reload` saved; with
//! `CLAP_PROBE_LOAD=<file>`, `reload` restores that state instead.
//! `CLAP_PROBE_PRESET=<file>` loads that instrument through CLAP preset-load
//! (into the first instance with `reload`, so the saved session holds it).

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{clap_event_header, clap_input_events, clap_output_events};
use clap_sys::ext::audio_ports::{CLAP_EXT_AUDIO_PORTS, clap_audio_port_info, clap_plugin_audio_ports};
use clap_sys::ext::preset_load::{CLAP_EXT_PRESET_LOAD, clap_plugin_preset_load};
use clap_sys::factory::preset_discovery::CLAP_PRESET_DISCOVERY_LOCATION_FILE;
use clap_sys::ext::gui::{CLAP_EXT_GUI, CLAP_WINDOW_API_X11, clap_plugin_gui, clap_window, clap_window_handle};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_plugin_state};
use clap_sys::factory::plugin_factory::{CLAP_PLUGIN_FACTORY_ID, clap_plugin_factory};
use clap_sys::host::clap_host;
use clap_sys::plugin::clap_plugin;
use clap_sys::process::clap_process;
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::CLAP_VERSION;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, ImageFormat, WindowClass};
use x11rb::rust_connection::RustConnection;

unsafe extern "C" fn get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    ptr::null()
}
unsafe extern "C" fn nothing(_: *const clap_host) {}

/// A window showing at least this many colours has drawn the editor.
const DRAWN: usize = 16;
const RATE: f64 = 48_000.0;
const BLOCK: u32 = 256;

fn fail(why: impl std::fmt::Display) -> ! {
    eprintln!("clap-probe: {why}");
    std::process::exit(1);
}

/// The window's pixels as RGB, and how many distinct colours they hold.
fn capture(conn: &impl Connection, window: u32, w: u16, h: u16) -> (Vec<u8>, usize) {
    let image = conn
        .get_image(ImageFormat::Z_PIXMAP, window, 0, 0, w, h, !0)
        .unwrap_or_else(|e| fail(e))
        .reply()
        .unwrap_or_else(|e| fail(e));
    let mut rgb = Vec::with_capacity(usize::from(w) * usize::from(h) * 3);
    for px in image.data.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let mut colors: Vec<[u8; 3]> = rgb.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    colors.sort_unstable();
    colors.dedup();
    (rgb, colors.len())
}

fn ext<T>(plugin: &clap_plugin, id: &CStr) -> Option<&'static T> {
    let p = unsafe { (plugin.get_extension?)(plugin, id.as_ptr()) } as *const T;
    (!p.is_null()).then(|| unsafe { &*p })
}

/// A plugin instance, created and initialised.
fn instance(factory: &clap_plugin_factory, host: &'static clap_host, id: *const c_char) -> &'static clap_plugin {
    let plugin = unsafe { (factory.create_plugin.unwrap())(factory, host, id) };
    if plugin.is_null() {
        fail("create_plugin refused");
    }
    let plugin = unsafe { &*plugin };
    if !unsafe { (plugin.init.unwrap())(plugin) } {
        fail("plugin init refused");
    }
    plugin
}

// --- state streams -------------------------------------------------------

unsafe extern "C" fn write(s: *const clap_ostream, buf: *const c_void, size: u64) -> i64 {
    let out = unsafe { &mut *((*s).ctx as *mut Vec<u8>) };
    out.extend_from_slice(unsafe { std::slice::from_raw_parts(buf as *const u8, size as usize) });
    size as i64
}

struct Reader {
    data: Vec<u8>,
    at: usize,
}

unsafe extern "C" fn read(s: *const clap_istream, buf: *mut c_void, size: u64) -> i64 {
    let r = unsafe { &mut *((*s).ctx as *mut Reader) };
    let n = (size as usize).min(r.data.len() - r.at);
    unsafe { ptr::copy_nonoverlapping(r.data.as_ptr().add(r.at), buf as *mut u8, n) };
    r.at += n;
    n as i64
}

/// Loads `CLAP_PROBE_PRESET`, if set, and gives the load time to settle.
fn load_preset(plugin: &clap_plugin) {
    let Some(file) = std::env::var_os("CLAP_PROBE_PRESET") else { return };
    let load: &clap_plugin_preset_load =
        ext(plugin, CLAP_EXT_PRESET_LOAD).unwrap_or_else(|| fail("no preset-load extension"));
    let path = CString::new(file.to_string_lossy().as_bytes()).unwrap_or_else(|e| fail(e));
    let ok = unsafe {
        (load.from_location.unwrap())(plugin, CLAP_PRESET_DISCOVERY_LOCATION_FILE, path.as_ptr(), ptr::null())
    };
    println!("preset-load {}: {ok}", file.to_string_lossy());
    for _ in 0..120 {
        unsafe { (plugin.on_main_thread.unwrap())(plugin) };
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn save_state(plugin: &clap_plugin) -> Vec<u8> {
    let state: &clap_plugin_state = ext(plugin, CLAP_EXT_STATE).unwrap_or_else(|| fail("no state extension"));
    let mut out = Vec::new();
    let stream = clap_ostream { ctx: &mut out as *mut Vec<u8> as *mut c_void, write: Some(write) };
    if !unsafe { (state.save.unwrap())(plugin, &stream) } {
        fail("state.save failed");
    }
    out
}

fn load_state(plugin: &clap_plugin, data: Vec<u8>) {
    let state: &clap_plugin_state = ext(plugin, CLAP_EXT_STATE).unwrap_or_else(|| fail("no state extension"));
    let mut reader = Reader { data, at: 0 };
    let stream = clap_istream { ctx: &mut reader as *mut Reader as *mut c_void, read: Some(read) };
    if !unsafe { (state.load.unwrap())(plugin, &stream) } {
        fail("state.load failed");
    }
}

// --- audio ---------------------------------------------------------------

unsafe extern "C" fn no_events(_: *const clap_input_events) -> u32 {
    0
}
unsafe extern "C" fn no_event(_: *const clap_input_events, _: u32) -> *const clap_event_header {
    ptr::null()
}
unsafe extern "C" fn drop_event(_: *const clap_output_events, _: *const clap_event_header) -> bool {
    true
}

struct Plugin(&'static clap_plugin);
unsafe impl Send for Plugin {}

/// Activates `plugin` and processes silent blocks on an audio thread, at
/// about real time, until `stop`.
fn start_audio(plugin: &'static clap_plugin, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    if !unsafe { (plugin.activate.unwrap())(plugin, RATE, 1, BLOCK) } {
        fail("activate failed");
    }
    let channels: Vec<u32> = ext::<clap_plugin_audio_ports>(plugin, CLAP_EXT_AUDIO_PORTS)
        .map(|ports| {
            let n = unsafe { (ports.count.unwrap())(plugin, false) };
            (0..n)
                .map(|i| {
                    let mut info: clap_audio_port_info = unsafe { std::mem::zeroed() };
                    unsafe { (ports.get.unwrap())(plugin, i, false, &mut info) };
                    info.channel_count
                })
                .collect()
        })
        .unwrap_or_default();
    let plugin = Plugin(plugin);
    std::thread::spawn(move || {
        let plugin = plugin.0;
        let mut samples: Vec<Vec<Vec<f32>>> =
            channels.iter().map(|&c| vec![vec![0.0; BLOCK as usize]; c as usize]).collect();
        let mut pointers: Vec<Vec<*mut f32>> =
            samples.iter_mut().map(|p| p.iter_mut().map(|c| c.as_mut_ptr()).collect()).collect();
        let mut outputs: Vec<clap_audio_buffer> = pointers
            .iter_mut()
            .map(|p| clap_audio_buffer {
                data32: p.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: p.len() as u32,
                latency: 0,
                constant_mask: 0,
            })
            .collect();
        let input = clap_input_events { ctx: ptr::null_mut(), size: Some(no_events), get: Some(no_event) };
        let output = clap_output_events { ctx: ptr::null_mut(), try_push: Some(drop_event) };
        if !unsafe { (plugin.start_processing.unwrap())(plugin) } {
            fail("start_processing failed");
        }
        let period = Duration::from_secs_f64(f64::from(BLOCK) / RATE);
        let mut steady = 0i64;
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            let process = clap_process {
                steady_time: steady,
                frames_count: BLOCK,
                transport: ptr::null(),
                audio_inputs: ptr::null(),
                audio_outputs: outputs.as_mut_ptr(),
                audio_inputs_count: 0,
                audio_outputs_count: outputs.len() as u32,
                in_events: &input,
                out_events: &output,
            };
            unsafe { (plugin.process.unwrap())(plugin, &process) };
            steady += i64::from(BLOCK);
            next += period;
            if let Some(wait) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        unsafe { (plugin.stop_processing.unwrap())(plugin) };
    })
}

// --- the editor ----------------------------------------------------------

/// Opens `plugin`'s editor in host windows shown per `mode`, waits until it
/// draws or `wait` runs out, and returns its window, size and whether it drew.
fn open_editor(
    conn: &RustConnection,
    screen: usize,
    plugin: &clap_plugin,
    mode: &str,
    show_after: Duration,
    wait: Duration,
) -> (u32, u16, u16, bool) {
    let gui: &clap_plugin_gui = ext(plugin, CLAP_EXT_GUI).unwrap_or_else(|| fail("no gui extension"));
    let started = Instant::now();
    unsafe {
        if !(gui.is_api_supported.unwrap())(plugin, CLAP_WINDOW_API_X11.as_ptr(), false) {
            fail("x11 embedding not supported");
        }
        if !(gui.create.unwrap())(plugin, CLAP_WINDOW_API_X11.as_ptr(), false) {
            fail("gui.create failed");
        }
    }
    let (mut w, mut h) = (0u32, 0u32);
    if !unsafe { (gui.get_size.unwrap())(plugin, &mut w, &mut h) } {
        fail("gui.get_size failed");
    }
    println!("editor size {w}x{h}");
    let (w, h) = (w as u16, h as u16);

    let screen = &conn.setup().roots[screen];
    let make = |parent: u32| {
        let id = conn.generate_id().unwrap_or_else(|e| fail(e));
        conn.create_window(
            screen.root_depth,
            id,
            parent,
            0,
            0,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.black_pixel),
        )
        .unwrap_or_else(|e| fail(e));
        id
    };
    let ok = |r: Result<_, x11rb::errors::ConnectionError>| {
        r.unwrap_or_else(|e| fail(e));
    };
    let top = make(screen.root);
    let window = if mode == "normal" { top } else { make(top) };
    if mode != "wm2" {
        ok(conn.map_window(window));
    }
    if matches!(mode, "reframe" | "flicker") {
        ok(conn.map_window(top));
    }
    conn.flush().unwrap_or_else(|e| fail(e));

    let parent = clap_window {
        api: CLAP_WINDOW_API_X11.as_ptr(),
        specific: clap_window_handle { x11: u64::from(window) },
    };
    unsafe {
        if !(gui.set_parent.unwrap())(plugin, &parent) {
            fail("gui.set_parent failed");
        }
        if !(gui.show.unwrap())(plugin) {
            fail("gui.show failed");
        }
    }
    println!("editor open after {:?}", started.elapsed());
    if mode != "normal" {
        std::thread::sleep(show_after);
        match mode {
            "late" => ok(conn.map_window(top)),
            "wm" | "wm2" => {
                if mode == "wm2" {
                    ok(conn.map_window(window));
                }
                let frame = make(screen.root);
                ok(conn.reparent_window(top, frame, 0, 0));
                ok(conn.map_window(top));
                ok(conn.map_window(frame));
            }
            "reframe" => {
                let frame = make(screen.root);
                ok(conn.reparent_window(top, frame, 0, 0));
                ok(conn.map_window(frame));
            }
            _ => {
                ok(conn.unmap_window(top));
                conn.flush().unwrap_or_else(|e| fail(e));
                std::thread::sleep(Duration::from_millis(20));
                ok(conn.map_window(top));
            }
        }
        conn.flush().unwrap_or_else(|e| fail(e));
        println!("host window shown after {:?}", started.elapsed());
    }

    // Until the editor has drawn (and a second more to settle), or the
    // deadline: a software renderer under load can take many seconds.
    let deadline = Instant::now() + wait;
    let (mut drawn, mut looked) = (None::<Instant>, Instant::now());
    while Instant::now() < deadline {
        if drawn.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) {
            break;
        }
        if drawn.is_none() && looked.elapsed() > Duration::from_millis(250) {
            looked = Instant::now();
            if capture(conn, window, w, h).1 >= DRAWN {
                drawn = Some(looked);
                println!("editor drew after {:?}", started.elapsed());
            }
        }
        while let Ok(Some(_)) = conn.poll_for_event() {}
        unsafe { (plugin.on_main_thread.unwrap())(plugin) };
        std::thread::sleep(Duration::from_millis(16));
    }
    (window, w, h, drawn.is_some())
}

fn close_editor(conn: &RustConnection, plugin: &clap_plugin, window: u32) {
    let gui: &clap_plugin_gui = ext(plugin, CLAP_EXT_GUI).unwrap_or_else(|| fail("no gui extension"));
    unsafe {
        (gui.hide.unwrap())(plugin);
        (gui.destroy.unwrap())(plugin);
    }
    let _ = conn.destroy_window(window);
    let _ = conn.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(path), Some(out)) = (args.get(1), args.get(2)) else {
        fail("usage: clap-probe <plugin.clap> <out.png> [max seconds] [mode] [show ms] [audio] [reload] [reopen]");
    };
    let wait = Duration::from_secs_f64(args.get(3).and_then(|s| s.parse::<f64>().ok()).unwrap_or(60.0));
    let mode = args.get(4).map_or("normal", String::as_str);
    if !["normal", "late", "wm", "wm2", "reframe", "flicker"].contains(&mode) {
        fail(format!("unknown mode {mode}"));
    }
    let show_after = Duration::from_millis(args.get(5).and_then(|s| s.parse().ok()).unwrap_or(500));
    let flags: Vec<&str> = args.iter().skip(6).map(String::as_str).collect();
    let (audio, reload, reopen) = (flags.contains(&"audio"), flags.contains(&"reload"), flags.contains(&"reopen"));
    if let Some(bad) = flags.iter().find(|f| !["audio", "reload", "reopen"].contains(f)) {
        fail(format!("unknown flag {bad}"));
    }

    let (conn, screen) = x11rb::connect(None).unwrap_or_else(|e| fail(format!("no X display: {e}")));
    // Leaked on purpose: the plugin stays loaded until exit, as in a host.
    let library = Box::leak(Box::new(unsafe { libloading::Library::new(path) }.unwrap_or_else(|e| fail(e))));
    let entry: &clap_plugin_entry = unsafe {
        &**library
            .get::<*const clap_plugin_entry>(b"clap_entry\0")
            .unwrap_or_else(|e| fail(e))
    };
    let cpath = CString::new(path.as_str()).unwrap_or_else(|e| fail(e));
    if !unsafe { (entry.init.unwrap())(cpath.as_ptr()) } {
        fail("clap_entry.init refused");
    }
    let factory = unsafe { (entry.get_factory.unwrap())(CLAP_PLUGIN_FACTORY_ID.as_ptr()) } as *const clap_plugin_factory;
    if factory.is_null() {
        fail("no plugin factory");
    }
    let factory = unsafe { &*factory };
    let descriptor = unsafe { &*(factory.get_plugin_descriptor.unwrap())(factory, 0) };
    println!("plugin {}", unsafe { CStr::from_ptr(descriptor.id) }.to_string_lossy());

    let host: &'static clap_host = Box::leak(Box::new(clap_host {
        clap_version: CLAP_VERSION,
        host_data: ptr::null_mut(),
        name: c"clap-probe".as_ptr(),
        vendor: c"CONTRA".as_ptr(),
        url: c"".as_ptr(),
        version: c"0".as_ptr(),
        get_extension: Some(get_extension),
        request_restart: Some(nothing),
        request_process: Some(nothing),
        request_callback: Some(nothing),
    }));

    // `reload`: the session as saved by an earlier instance.
    let saved = reload.then(|| {
        if let Some(file) = std::env::var_os("CLAP_PROBE_LOAD") {
            let state = std::fs::read(&file).unwrap_or_else(|e| fail(e));
            println!("loaded a session from {}: {} bytes", file.to_string_lossy(), state.len());
            return state;
        }
        let first = instance(factory, host, descriptor.id);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = audio.then(|| start_audio(first, Arc::clone(&stop)));
        load_preset(first);
        std::thread::sleep(Duration::from_millis(300));
        let state = save_state(first);
        stop.store(true, Ordering::Relaxed);
        if let Some(t) = thread {
            let _ = t.join();
            unsafe { (first.deactivate.unwrap())(first) };
        }
        unsafe { (first.destroy.unwrap())(first) };
        println!("saved a session: {} bytes of state", state.len());
        if let Some(file) = std::env::var_os("CLAP_PROBE_SAVE") {
            std::fs::write(file, &state).unwrap_or_else(|e| fail(e));
        }
        state
    });

    let plugin = instance(factory, host, descriptor.id);
    if let Some(state) = saved {
        load_state(plugin, state);
        println!("restored the session into a new instance");
    }
    let stop = Arc::new(AtomicBool::new(false));
    let audio_thread = audio.then(|| start_audio(plugin, Arc::clone(&stop)));
    if !reload {
        load_preset(plugin);
    }

    let (mut window, mut w, mut h, mut drew) = open_editor(&conn, screen, plugin, mode, show_after, wait);
    if reopen && drew {
        close_editor(&conn, plugin, window);
        println!("closed the editor; opening it again");
        (window, w, h, drew) = open_editor(&conn, screen, plugin, mode, show_after, wait);
    }

    let (rgb, colors) = capture(&conn, window, w, h);
    let file = std::fs::File::create(out).unwrap_or_else(|e| fail(e));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), u32::from(w), u32::from(h));
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgb))
        .unwrap_or_else(|e| fail(e));
    println!("wrote {out}: {colors} distinct colours");

    close_editor(&conn, plugin, window);
    stop.store(true, Ordering::Relaxed);
    if let Some(t) = audio_thread {
        let _ = t.join();
        unsafe { (plugin.deactivate.unwrap())(plugin) };
    }
    unsafe {
        (plugin.destroy.unwrap())(plugin);
        (entry.deinit.unwrap())();
    }
    println!("closed cleanly");
    if !drew || colors < DRAWN {
        fail("the editor drew nothing");
    }
}
