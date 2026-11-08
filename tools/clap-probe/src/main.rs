//! `clap-probe <plugin.clap> <out.png> [seconds]`: loads a CLAP plugin the
//! way a DAW does (no environment help), embeds its editor in an X11 window
//! of the size it asks for, waits, and writes what the window shows. Exits
//! non-zero when the editor does not open or draws nothing.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;
use std::time::{Duration, Instant};

use clap_sys::entry::clap_plugin_entry;
use clap_sys::ext::gui::{CLAP_EXT_GUI, CLAP_WINDOW_API_X11, clap_plugin_gui, clap_window, clap_window_handle};
use clap_sys::factory::plugin_factory::{CLAP_PLUGIN_FACTORY_ID, clap_plugin_factory};
use clap_sys::host::clap_host;
use clap_sys::version::CLAP_VERSION;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, ImageFormat, WindowClass};

unsafe extern "C" fn get_extension(_: *const clap_host, _: *const c_char) -> *const c_void {
    ptr::null()
}
unsafe extern "C" fn nothing(_: *const clap_host) {}

fn fail(why: impl std::fmt::Display) -> ! {
    eprintln!("clap-probe: {why}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(path), Some(out)) = (args.get(1), args.get(2)) else {
        fail("usage: clap-probe <plugin.clap> <out.png> [seconds]");
    };
    let wait = args.get(3).and_then(|s| s.parse::<f64>().ok()).unwrap_or(5.0);

    let (conn, screen) = x11rb::connect(None).unwrap_or_else(|e| fail(format!("no X display: {e}")));
    // Leaked on purpose: the plugin stays loaded until exit, as in a host.
    let library = Box::leak(Box::new(unsafe { libloading::Library::new(path) }.unwrap_or_else(|e| fail(e))));
    let entry: &clap_plugin_entry = unsafe {
        &**library
            .get::<*const clap_plugin_entry>(b"clap_entry\0")
            .unwrap_or_else(|e| fail(e))
    };
    let cpath = CString::new(path.as_str()).unwrap_or_else(|e| fail(e));
    unsafe {
        if !(entry.init.unwrap())(cpath.as_ptr()) {
            fail("clap_entry.init refused");
        }
    }
    let factory = unsafe { (entry.get_factory.unwrap())(CLAP_PLUGIN_FACTORY_ID.as_ptr()) } as *const clap_plugin_factory;
    if factory.is_null() {
        fail("no plugin factory");
    }
    let factory = unsafe { &*factory };
    let descriptor = unsafe { &*(factory.get_plugin_descriptor.unwrap())(factory, 0) };
    let id = unsafe { CStr::from_ptr(descriptor.id) };
    println!("plugin {}", id.to_string_lossy());

    let host = Box::leak(Box::new(clap_host {
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
    let plugin = unsafe { (factory.create_plugin.unwrap())(factory, host, descriptor.id) };
    if plugin.is_null() {
        fail("create_plugin refused");
    }
    let plugin = unsafe { &*plugin };
    unsafe {
        if !(plugin.init.unwrap())(plugin) {
            fail("plugin init refused");
        }
    }
    let gui = unsafe { (plugin.get_extension.unwrap())(plugin, CLAP_EXT_GUI.as_ptr()) } as *const clap_plugin_gui;
    if gui.is_null() {
        fail("no gui extension");
    }
    let gui = unsafe { &*gui };
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
    unsafe {
        if !(gui.get_size.unwrap())(plugin, &mut w, &mut h) {
            fail("gui.get_size failed");
        }
    }
    println!("editor size {w}x{h}");

    let screen = &conn.setup().roots[screen];
    let window = conn.generate_id().unwrap_or_else(|e| fail(e));
    conn.create_window(
        screen.root_depth,
        window,
        screen.root,
        0,
        0,
        w as u16,
        h as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new().background_pixel(screen.black_pixel),
    )
    .unwrap_or_else(|e| fail(e));
    conn.map_window(window).unwrap_or_else(|e| fail(e));
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

    let deadline = Instant::now() + Duration::from_secs_f64(wait);
    while Instant::now() < deadline {
        while let Ok(Some(_)) = conn.poll_for_event() {}
        unsafe { (plugin.on_main_thread.unwrap())(plugin) };
        std::thread::sleep(Duration::from_millis(16));
    }

    // What the screen shows inside the window: the plugin's child included.
    let image = conn
        .get_image(ImageFormat::Z_PIXMAP, window, 0, 0, w as u16, h as u16, !0)
        .unwrap_or_else(|e| fail(e))
        .reply()
        .unwrap_or_else(|e| fail(e));
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in image.data.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let file = std::fs::File::create(out).unwrap_or_else(|e| fail(e));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgb))
        .unwrap_or_else(|e| fail(e));
    let mut colors: Vec<[u8; 3]> = rgb.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    colors.sort_unstable();
    colors.dedup();
    println!("wrote {out}: {} distinct colours", colors.len());

    unsafe {
        (gui.hide.unwrap())(plugin);
        (gui.destroy.unwrap())(plugin);
        (plugin.destroy.unwrap())(plugin);
        (entry.deinit.unwrap())();
    }
    println!("closed cleanly");
    if colors.len() < 16 {
        fail("the editor drew nothing");
    }
}
