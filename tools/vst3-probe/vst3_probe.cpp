// vst3-probe <plugin.vst3 bundle or .so> <out.png> [seconds]
//
// Loads a VST3 plugin the way a Linux DAW does (dlopen, ModuleEntry,
// GetPluginFactory), opens its editor in an X11 window through
// IPlugView::attached("X11EmbedWindowID"), serves the host IRunLoop's
// timers, waits, and writes what the window shows. Exits non-zero when the
// editor does not open or draws nothing. No VST3 SDK: the ABI is spelled
// out here, as in vendor/moose-vst3/shim/vst3_shim.cpp.

#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <dlfcn.h>
#include <png.h>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <set>
#include <string>
#include <thread>
#include <vector>

typedef int32_t tresult;
typedef int8_t TUID[16];
static const tresult kResultOk = 0;
static const tresult kNoInterface = -1;

#define I(x) static_cast<int8_t>(x)
#define MAKE_IID(l1, l2, l3, l4) { \
    I(((l1) >> 24) & 0xFF), I(((l1) >> 16) & 0xFF), I(((l1) >> 8) & 0xFF), I((l1) & 0xFF), \
    I(((l2) >> 24) & 0xFF), I(((l2) >> 16) & 0xFF), I(((l2) >> 8) & 0xFF), I((l2) & 0xFF), \
    I(((l3) >> 24) & 0xFF), I(((l3) >> 16) & 0xFF), I(((l3) >> 8) & 0xFF), I((l3) & 0xFF), \
    I(((l4) >> 24) & 0xFF), I(((l4) >> 16) & 0xFF), I(((l4) >> 8) & 0xFF), I((l4) & 0xFF) }

static const TUID FUnknown_iid = MAKE_IID(0x00000000, 0x00000000, 0xC0000000, 0x00000046);
static const TUID IComponent_iid = MAKE_IID(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
static const TUID IEditController_iid = MAKE_IID(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
static const TUID IPlugFrame_iid = MAKE_IID(0x367FAF01, 0xAFA94693, 0x8D4DA2A0, 0xED0882A3);
static const TUID IRunLoop_iid = MAKE_IID(0x18C35366, 0x97764F1A, 0x9C5B8385, 0x7A871389);

struct PClassInfo {
    TUID cid;
    int32_t cardinality;
    char category[32];
    char name[64];
};
struct ViewRect { int32_t left, top, right, bottom; };

template <typename F>
static F slot(void* obj, int index) { return reinterpret_cast<F>((*reinterpret_cast<void***>(obj))[index]); }

static bool same(const TUID a, const TUID b) { return std::memcmp(a, b, 16) == 0; }

[[noreturn]] static void fail(const std::string& why) {
    std::fprintf(stderr, "vst3-probe: %s\n", why.c_str());
    std::exit(1);
}

// The host's run loop: timers the plugin registers, fired from main().
struct Timer { void* handler; uint64_t ms; std::chrono::steady_clock::time_point next; };
static std::vector<Timer> g_timers;

struct RunLoop { void** vtbl; };
static tresult rl_qi(void* self, const TUID iid, void** out) {
    if (same(iid, FUnknown_iid) || same(iid, IRunLoop_iid)) { *out = self; return kResultOk; }
    *out = nullptr; return kNoInterface;
}
static uint32_t rl_ref(void*) { return 1; }
static tresult rl_register_event(void*, void*, int) { return kResultOk; }
static tresult rl_unregister_event(void*, void*) { return kResultOk; }
static tresult rl_register_timer(void*, void* handler, uint64_t ms) {
    g_timers.push_back({handler, ms, std::chrono::steady_clock::now()});
    return kResultOk;
}
static tresult rl_unregister_timer(void*, void* handler) {
    for (auto it = g_timers.begin(); it != g_timers.end(); ++it)
        if (it->handler == handler) { g_timers.erase(it); break; }
    return kResultOk;
}
static void* g_rl_vtbl[] = {(void*)rl_qi, (void*)rl_ref, (void*)rl_ref, (void*)rl_register_event,
                            (void*)rl_unregister_event, (void*)rl_register_timer, (void*)rl_unregister_timer};
static RunLoop g_run_loop{g_rl_vtbl};

// The plug frame: hands out the run loop, accepts resizes.
struct Frame { void** vtbl; };
static tresult fr_qi(void* self, const TUID iid, void** out) {
    if (same(iid, FUnknown_iid) || same(iid, IPlugFrame_iid)) { *out = self; return kResultOk; }
    if (same(iid, IRunLoop_iid)) { *out = &g_run_loop; return kResultOk; }
    *out = nullptr; return kNoInterface;
}
static uint32_t fr_ref(void*) { return 1; }
static tresult fr_resize(void*, void*, ViewRect* r) {
    std::printf("resizeView %dx%d\n", r->right - r->left, r->bottom - r->top);
    return kResultOk;
}
static void* g_fr_vtbl[] = {(void*)fr_qi, (void*)fr_ref, (void*)fr_ref, (void*)fr_resize};
static Frame g_frame{g_fr_vtbl};

// The host application context passed to initialize(): answers nothing.
struct HostApp { void** vtbl; };
static tresult ha_qi(void*, const TUID, void** out) { *out = nullptr; return kNoInterface; }
static uint32_t ha_ref(void*) { return 1; }
static void* g_ha_vtbl[] = {(void*)ha_qi, (void*)ha_ref, (void*)ha_ref};
static HostApp g_host{g_ha_vtbl};

static void write_png(const char* path, const std::vector<uint8_t>& rgb, int w, int h) {
    FILE* f = std::fopen(path, "wb");
    if (!f) fail("cannot write png");
    png_structp png = png_create_write_struct(PNG_LIBPNG_VER_STRING, nullptr, nullptr, nullptr);
    png_infop info = png_create_info_struct(png);
    png_init_io(png, f);
    png_set_IHDR(png, info, w, h, 8, PNG_COLOR_TYPE_RGB, PNG_INTERLACE_NONE, PNG_COMPRESSION_TYPE_DEFAULT,
                 PNG_FILTER_TYPE_DEFAULT);
    png_write_info(png, info);
    for (int y = 0; y < h; ++y) png_write_row(png, const_cast<uint8_t*>(&rgb[size_t(y) * w * 3]));
    png_write_end(png, nullptr);
    png_destroy_write_struct(&png, &info);
    std::fclose(f);
}

int main(int argc, char** argv) {
    if (argc < 3) fail("usage: vst3-probe <plugin.vst3|.so> <out.png> [seconds]");
    std::string path = argv[1];
    if (path.size() > 5 && path.compare(path.size() - 5, 5, ".vst3") == 0) {
        std::string stem = path.substr(path.find_last_of('/') + 1);
        stem = stem.substr(0, stem.size() - 5);
        path += "/Contents/x86_64-linux/" + stem + ".so";
    }
    double wait = argc > 3 ? std::atof(argv[3]) : 5.0;

    Display* dpy = XOpenDisplay(nullptr);
    if (!dpy) fail("no X display");

    void* lib = dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
    if (!lib) fail(std::string("dlopen: ") + dlerror());
    if (auto entry = reinterpret_cast<bool (*)(void*)>(dlsym(lib, "ModuleEntry")))
        if (!entry(lib)) fail("ModuleEntry refused");
    auto get_factory = reinterpret_cast<void* (*)()>(dlsym(lib, "GetPluginFactory"));
    if (!get_factory) fail("no GetPluginFactory");
    void* factory = get_factory();
    if (!factory) fail("GetPluginFactory returned null");

    int32_t classes = slot<int32_t (*)(void*)>(factory, 4)(factory);
    PClassInfo info{};
    bool found = false;
    for (int32_t n = 0; n < classes; ++n) {
        slot<tresult (*)(void*, int32_t, PClassInfo*)>(factory, 5)(factory, n, &info);
        if (std::strcmp(info.category, "Audio Module Class") == 0) { found = true; break; }
    }
    if (!found) fail("no audio module class");
    std::printf("plugin %s\n", info.name);

    void* component = nullptr;
    if (slot<tresult (*)(void*, const char*, const char*, void**)>(factory, 6)(
            factory, reinterpret_cast<const char*>(info.cid), reinterpret_cast<const char*>(IComponent_iid),
            &component) != kResultOk || !component)
        fail("createInstance(IComponent) failed");
    if (slot<tresult (*)(void*, void*)>(component, 3)(component, &g_host) != kResultOk)
        fail("IComponent::initialize failed");

    // A single-component plugin answers IEditController itself; otherwise
    // the controller is its own class, created and initialised separately.
    void* controller = nullptr;
    if (slot<tresult (*)(void*, const TUID, void**)>(component, 0)(component, IEditController_iid, &controller) !=
            kResultOk || !controller) {
        TUID cid;
        if (slot<tresult (*)(void*, TUID)>(component, 5)(component, cid) != kResultOk) fail("no controller class");
        if (slot<tresult (*)(void*, const char*, const char*, void**)>(factory, 6)(
                factory, reinterpret_cast<const char*>(cid), reinterpret_cast<const char*>(IEditController_iid),
                &controller) != kResultOk || !controller)
            fail("createInstance(IEditController) failed");
        if (slot<tresult (*)(void*, void*)>(controller, 3)(controller, &g_host) != kResultOk)
            fail("IEditController::initialize failed");
    }

    auto started = std::chrono::steady_clock::now();
    void* view = slot<void* (*)(void*, const char*)>(controller, 17)(controller, "editor");
    if (!view) fail("createView returned null");
    if (slot<tresult (*)(void*, const char*)>(view, 3)(view, "X11EmbedWindowID") != kResultOk)
        fail("X11EmbedWindowID not supported");
    slot<tresult (*)(void*, void*)>(view, 12)(view, &g_frame);
    ViewRect rect{};
    if (slot<tresult (*)(void*, ViewRect*)>(view, 9)(view, &rect) != kResultOk) fail("getSize failed");
    int w = rect.right - rect.left, h = rect.bottom - rect.top;
    std::printf("editor size %dx%d\n", w, h);

    int screen = DefaultScreen(dpy);
    Window window = XCreateSimpleWindow(dpy, RootWindow(dpy, screen), 0, 0, w, h, 0, BlackPixel(dpy, screen),
                                        BlackPixel(dpy, screen));
    XMapWindow(dpy, window);
    XSync(dpy, False);
    if (slot<tresult (*)(void*, void*, const char*)>(view, 4)(view, reinterpret_cast<void*>(window),
                                                              "X11EmbedWindowID") != kResultOk)
        fail("IPlugView::attached failed");
    std::printf("editor attached after %.1f ms\n",
                std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - started).count());

    auto deadline = std::chrono::steady_clock::now() + std::chrono::duration<double>(wait);
    while (std::chrono::steady_clock::now() < deadline) {
        while (XPending(dpy)) { XEvent e; XNextEvent(dpy, &e); }
        auto now = std::chrono::steady_clock::now();
        for (auto t : std::vector<Timer>(g_timers))
            if (now >= t.next) {
                slot<void (*)(void*)>(t.handler, 3)(t.handler);  // ITimerHandler::onTimer
                for (auto& u : g_timers)
                    if (u.handler == t.handler) u.next = now + std::chrono::milliseconds(t.ms);
            }
        std::this_thread::sleep_for(std::chrono::milliseconds(16));
    }

    XImage* image = XGetImage(dpy, window, 0, 0, w, h, AllPlanes, ZPixmap);
    if (!image) fail("XGetImage failed");
    std::vector<uint8_t> rgb(size_t(w) * h * 3);
    std::set<uint32_t> colors;
    for (int y = 0; y < h; ++y)
        for (int x = 0; x < w; ++x) {
            unsigned long p = XGetPixel(image, x, y);
            uint8_t* o = &rgb[(size_t(y) * w + x) * 3];
            o[0] = (p >> 16) & 0xFF; o[1] = (p >> 8) & 0xFF; o[2] = p & 0xFF;
            colors.insert(uint32_t(p & 0xFFFFFF));
        }
    XDestroyImage(image);
    write_png(argv[2], rgb, w, h);
    std::printf("wrote %s: %zu distinct colours\n", argv[2], colors.size());

    slot<tresult (*)(void*)>(view, 5)(view);              // removed
    slot<uint32_t (*)(void*)>(view, 2)(view);             // release
    if (controller != component) {
        slot<tresult (*)(void*)>(controller, 4)(controller);
        slot<uint32_t (*)(void*)>(controller, 2)(controller);
    } else {
        slot<uint32_t (*)(void*)>(controller, 2)(controller);
    }
    slot<tresult (*)(void*)>(component, 4)(component);    // terminate
    slot<uint32_t (*)(void*)>(component, 2)(component);
    slot<uint32_t (*)(void*)>(factory, 2)(factory);
    if (auto exit_fn = reinterpret_cast<bool (*)()>(dlsym(lib, "ModuleExit"))) exit_fn();
    std::printf("closed cleanly\n");
    if (colors.size() < 16) fail("the editor drew nothing");
    return 0;
}
