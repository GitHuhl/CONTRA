//! `vst3-probe <plugin.vst3 bundle or .so> <out.png> [max seconds]`: loads a VST3
//! plugin the way a Linux DAW does (dlopen, ModuleEntry, GetPluginFactory),
//! opens its editor in an X11 window through
//! `IPlugView::attached("X11EmbedWindowID")`, serves the host `IRunLoop`'s
//! timers, waits, and writes what the window shows. Exits non-zero when the
//! editor does not open or draws nothing.
//!
//! A fourth argument, `late` or `wm`, embeds the editor as clap-probe does
//! with it: in a container inside a hidden top-level the host shows only
//! after attaching, and with `wm` shows it as a reparenting window manager
//! does (moved into a frame, mapped, frame mapped). No VST3 SDK: the COM ABI is
//! spelled out here (vtables of function pointers, IIDs in the non-Windows
//! byte order), as in vendor/moose-vst3/shim/vst3_shim.cpp.

use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, ImageFormat, WindowClass};
use x11rb::wrapper::ConnectionExt as _;

type TResult = i32;
type Tuid = [u8; 16];
const OK: TResult = 0;
const NO_INTERFACE: TResult = -1;

const fn iid(a: u32, b: u32, c: u32, d: u32) -> Tuid {
    let (a, b, c, d) = (
        a.to_be_bytes(),
        b.to_be_bytes(),
        c.to_be_bytes(),
        d.to_be_bytes(),
    );
    [
        a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3], d[0], d[1], d[2],
        d[3],
    ]
}
const FUNKNOWN: Tuid = iid(0x00000000, 0x00000000, 0xC0000000, 0x00000046);
const ICOMPONENT: Tuid = iid(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
const IEDIT_CONTROLLER: Tuid = iid(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
const IPLUG_FRAME: Tuid = iid(0x367FAF01, 0xAFA94693, 0x8D4DA2A0, 0xED0882A3);
const IRUN_LOOP: Tuid = iid(0x18C35366, 0x97764F1A, 0x9C5B8385, 0x7A871389);

#[repr(C)]
struct PClassInfo {
    cid: Tuid,
    cardinality: i32,
    category: [c_char; 32],
    name: [c_char; 64],
}

#[repr(C)]
#[derive(Default)]
struct ViewRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Method `index` of the COM object `obj`.
unsafe fn method<F: Copy>(obj: *mut c_void, index: usize) -> F {
    unsafe {
        let vtbl = *(obj as *const *const *const c_void);
        std::mem::transmute_copy(&*vtbl.add(index))
    }
}

/// A window showing at least this many colours has drawn the editor.
const DRAWN: usize = 16;

fn fail(why: impl std::fmt::Display) -> ! {
    eprintln!("vst3-probe: {why}");
    std::process::exit(1);
}

// The host objects: each is a pointer to its vtable.
#[repr(C)]
struct Object {
    vtbl: *const *const c_void,
}
unsafe impl Sync for Object {}

/// Timers the plugin registered with the run loop: (handler, period, next).
static TIMERS: Mutex<Vec<(usize, Duration, Instant)>> = Mutex::new(Vec::new());

unsafe extern "C" fn add_ref(_: *mut c_void) -> u32 {
    1
}
unsafe extern "C" fn no_interface(
    _: *mut c_void,
    _: *const Tuid,
    out: *mut *mut c_void,
) -> TResult {
    unsafe { *out = std::ptr::null_mut() };
    NO_INTERFACE
}

unsafe extern "C" fn run_loop_qi(
    this: *mut c_void,
    iid: *const Tuid,
    out: *mut *mut c_void,
) -> TResult {
    let iid = unsafe { &*iid };
    unsafe {
        *out = if *iid == FUNKNOWN || *iid == IRUN_LOOP {
            this
        } else {
            std::ptr::null_mut()
        }
    };
    if unsafe { (*out).is_null() } {
        NO_INTERFACE
    } else {
        OK
    }
}
unsafe extern "C" fn register_event(_: *mut c_void, _: *mut c_void, _: i32) -> TResult {
    OK
}
unsafe extern "C" fn unregister_event(_: *mut c_void, _: *mut c_void) -> TResult {
    OK
}
unsafe extern "C" fn register_timer(_: *mut c_void, handler: *mut c_void, ms: u64) -> TResult {
    let mut timers = TIMERS.lock().unwrap_or_else(|e| e.into_inner());
    timers.push((handler as usize, Duration::from_millis(ms), Instant::now()));
    OK
}
unsafe extern "C" fn unregister_timer(_: *mut c_void, handler: *mut c_void) -> TResult {
    let mut timers = TIMERS.lock().unwrap_or_else(|e| e.into_inner());
    timers.retain(|t| t.0 != handler as usize);
    OK
}
struct Vtbl<const N: usize>([*const c_void; N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}
static RUN_LOOP_VTBL: Vtbl<7> = Vtbl([
    run_loop_qi as *const c_void,
    add_ref as *const c_void,
    add_ref as *const c_void,
    register_event as *const c_void,
    unregister_event as *const c_void,
    register_timer as *const c_void,
    unregister_timer as *const c_void,
]);
static RUN_LOOP: Object = Object {
    vtbl: RUN_LOOP_VTBL.0.as_ptr(),
};

unsafe extern "C" fn frame_qi(
    this: *mut c_void,
    iid: *const Tuid,
    out: *mut *mut c_void,
) -> TResult {
    let iid = unsafe { &*iid };
    let found = if *iid == FUNKNOWN || *iid == IPLUG_FRAME {
        this
    } else if *iid == IRUN_LOOP {
        &RUN_LOOP as *const Object as *mut c_void
    } else {
        std::ptr::null_mut()
    };
    unsafe { *out = found };
    if found.is_null() { NO_INTERFACE } else { OK }
}
unsafe extern "C" fn resize_view(_: *mut c_void, _: *mut c_void, r: *mut ViewRect) -> TResult {
    let r = unsafe { &*r };
    println!("resizeView {}x{}", r.right - r.left, r.bottom - r.top);
    OK
}
static FRAME_VTBL: Vtbl<4> = Vtbl([
    frame_qi as *const c_void,
    add_ref as *const c_void,
    add_ref as *const c_void,
    resize_view as *const c_void,
]);
static FRAME: Object = Object {
    vtbl: FRAME_VTBL.0.as_ptr(),
};

/// The host application context passed to `initialize`: answers nothing.
static HOST_VTBL: Vtbl<3> = Vtbl([
    no_interface as *const c_void,
    add_ref as *const c_void,
    add_ref as *const c_void,
]);
static HOST: Object = Object {
    vtbl: HOST_VTBL.0.as_ptr(),
};

fn ptr(o: &'static Object) -> *mut c_void {
    o as *const Object as *mut c_void
}

type Qi = unsafe extern "C" fn(*mut c_void, *const Tuid, *mut *mut c_void) -> TResult;
type Release = unsafe extern "C" fn(*mut c_void) -> u32;
type Initialize = unsafe extern "C" fn(*mut c_void, *mut c_void) -> TResult;
type Terminate = unsafe extern "C" fn(*mut c_void) -> TResult;
type CreateInstance =
    unsafe extern "C" fn(*mut c_void, *const Tuid, *const Tuid, *mut *mut c_void) -> TResult;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(path), Some(out)) = (args.get(1), args.get(2)) else {
        fail("usage: vst3-probe <plugin.vst3|.so> <out.png> [max seconds]");
    };
    let wait = args
        .get(3)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(60.0);
    let wm = args.get(4).is_some_and(|a| a == "wm");
    let late = wm || args.get(4).is_some_and(|a| a == "late");
    let mut path = path.clone();
    if let Some(stem) = path.strip_suffix(".vst3").map(str::to_owned) {
        let name = stem.rsplit('/').next().unwrap_or(&stem).to_owned();
        path = format!("{path}/Contents/x86_64-linux/{name}.so");
    }

    let (conn, screen) =
        x11rb::connect(None).unwrap_or_else(|e| fail(format!("no X display: {e}")));
    // Leaked on purpose: the plugin stays loaded until exit, as in a host.
    let lib = Box::leak(Box::new(
        unsafe { libloading::Library::new(&path) }.unwrap_or_else(|e| fail(e)),
    ));
    unsafe {
        if let Ok(entry) = lib.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0") {
            if !entry(std::ptr::null_mut()) {
                fail("ModuleEntry refused");
            }
        }
    }
    let factory = unsafe {
        lib.get::<unsafe extern "C" fn() -> *mut c_void>(b"GetPluginFactory\0")
            .unwrap_or_else(|e| fail(e))()
    };
    if factory.is_null() {
        fail("GetPluginFactory returned null");
    }

    let classes =
        unsafe { method::<unsafe extern "C" fn(*mut c_void) -> i32>(factory, 4)(factory) };
    let mut info: PClassInfo = unsafe { std::mem::zeroed() };
    let found = (0..classes).any(|n| unsafe {
        method::<unsafe extern "C" fn(*mut c_void, i32, *mut PClassInfo) -> TResult>(factory, 5)(
            factory, n, &mut info,
        );
        CStr::from_ptr(info.category.as_ptr()).to_bytes() == b"Audio Module Class"
    });
    if !found {
        fail("no audio module class");
    }
    println!(
        "plugin {}",
        unsafe { CStr::from_ptr(info.name.as_ptr()) }.to_string_lossy()
    );

    let create = unsafe { method::<CreateInstance>(factory, 6) };
    let mut component = std::ptr::null_mut();
    if unsafe { create(factory, &info.cid, &ICOMPONENT, &mut component) } != OK
        || component.is_null()
    {
        fail("createInstance(IComponent) failed");
    }
    if unsafe { method::<Initialize>(component, 3)(component, ptr(&HOST)) } != OK {
        fail("IComponent::initialize failed");
    }
    // A single-component plugin answers IEditController itself; otherwise
    // the controller is its own class, created and initialised separately.
    let mut controller = std::ptr::null_mut();
    let separate =
        unsafe { method::<Qi>(component, 0)(component, &IEDIT_CONTROLLER, &mut controller) } != OK
            || controller.is_null();
    if separate {
        let mut cid: Tuid = [0; 16];
        if unsafe {
            method::<unsafe extern "C" fn(*mut c_void, *mut Tuid) -> TResult>(component, 5)(
                component, &mut cid,
            )
        } != OK
        {
            fail("no controller class");
        }
        if unsafe { create(factory, &cid, &IEDIT_CONTROLLER, &mut controller) } != OK
            || controller.is_null()
        {
            fail("createInstance(IEditController) failed");
        }
        if unsafe { method::<Initialize>(controller, 3)(controller, ptr(&HOST)) } != OK {
            fail("IEditController::initialize failed");
        }
    }

    let started = Instant::now();
    let view = unsafe {
        method::<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>(controller, 17)(
            controller,
            c"editor".as_ptr(),
        )
    };
    if view.is_null() {
        fail("createView returned null");
    }
    let x11 = c"X11EmbedWindowID".as_ptr();
    unsafe {
        if method::<unsafe extern "C" fn(*mut c_void, *const c_char) -> TResult>(view, 3)(view, x11)
            != OK
        {
            fail("X11EmbedWindowID not supported");
        }
        method::<unsafe extern "C" fn(*mut c_void, *mut c_void) -> TResult>(view, 12)(
            view,
            ptr(&FRAME),
        );
    }
    let mut rect = ViewRect::default();
    if unsafe {
        method::<unsafe extern "C" fn(*mut c_void, *mut ViewRect) -> TResult>(view, 9)(
            view, &mut rect,
        )
    } != OK
    {
        fail("getSize failed");
    }
    let (w, h) = (
        (rect.right - rect.left) as u16,
        (rect.bottom - rect.top) as u16,
    );
    println!("editor size {w}x{h}");

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
    let top = make(screen.root);
    let window = if late { make(top) } else { top };
    conn.map_window(window).unwrap_or_else(|e| fail(e));
    conn.sync().unwrap_or_else(|e| fail(e));
    let attached =
        unsafe {
            method::<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> TResult>(
                view, 4,
            )(view, window as usize as *mut c_void, x11)
        };
    if attached != OK {
        fail("IPlugView::attached failed");
    }
    println!("editor attached after {:?}", started.elapsed());
    if late {
        std::thread::sleep(Duration::from_millis(500));
        if wm {
            let frame = make(screen.root);
            conn.reparent_window(top, frame, 0, 0)
                .unwrap_or_else(|e| fail(e));
            conn.map_window(top).unwrap_or_else(|e| fail(e));
            conn.map_window(frame).unwrap_or_else(|e| fail(e));
        } else {
            conn.map_window(top).unwrap_or_else(|e| fail(e));
        }
        conn.flush().unwrap_or_else(|e| fail(e));
        println!("host window shown after {:?}", started.elapsed());
    }

    // Until the editor has drawn (and a second more to settle), or the
    // deadline: a software renderer under load can take many seconds.
    let deadline = Instant::now() + Duration::from_secs_f64(wait);
    let (mut drawn, mut looked) = (None::<Instant>, Instant::now());
    while Instant::now() < deadline {
        if drawn.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) {
            break;
        }
        if drawn.is_none() && looked.elapsed() > Duration::from_millis(250) {
            looked = Instant::now();
            if capture(&conn, window, w, h).1 >= DRAWN {
                drawn = Some(looked);
                println!("editor drew after {:?}", started.elapsed());
            }
        }
        while let Ok(Some(_)) = conn.poll_for_event() {}
        let now = Instant::now();
        let due: Vec<usize> = {
            let mut timers = TIMERS.lock().unwrap_or_else(|e| e.into_inner());
            timers
                .iter_mut()
                .filter(|t| now >= t.2)
                .map(|t| {
                    t.2 = now + t.1;
                    t.0
                })
                .collect()
        };
        for handler in due {
            // ITimerHandler::onTimer
            unsafe {
                method::<unsafe extern "C" fn(*mut c_void)>(handler as *mut c_void, 3)(
                    handler as *mut c_void,
                )
            };
        }
        std::thread::sleep(Duration::from_millis(16));
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

    unsafe {
        method::<Terminate>(view, 5)(view); // removed
        method::<Release>(view, 2)(view);
        if separate {
            method::<Terminate>(controller, 4)(controller);
        }
        method::<Release>(controller, 2)(controller);
        method::<Terminate>(component, 4)(component);
        method::<Release>(component, 2)(component);
        method::<Release>(factory, 2)(factory);
        if let Ok(exit) = lib.get::<unsafe extern "C" fn() -> bool>(b"ModuleExit\0") {
            exit();
        }
    }
    println!("closed cleanly");
    if colors < DRAWN {
        fail("the editor drew nothing");
    }
}
