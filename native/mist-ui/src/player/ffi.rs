// libmpv, libEGL and libwayland-egl loaded at run time (dlopen): only the player process maps them, the shell
// binary keeps no link to them, and a missing mpv is an error message instead of a broken mist-ui-bin.
#![allow(non_camel_case_types, non_snake_case, dead_code, clippy::missing_safety_doc)]
use std::ffi::{c_char, c_int, c_void, CStr, CString};

pub type Handle = *mut c_void;

fn lib(name: &str) -> Result<*mut c_void, String> {
    let c = CString::new(name).unwrap();
    let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if h.is_null() { Err(format!("brak {name} (pacman -S mpv)")) } else { Ok(h) }
}
unsafe fn sym<T: Copy>(h: *mut c_void, name: &str) -> Result<T, String> {
    let c = CString::new(name).unwrap();
    let p = unsafe { libc::dlsym(h, c.as_ptr()) };
    if p.is_null() {
        return Err(format!("brak symbolu {name}"));
    }
    Ok(unsafe { std::mem::transmute_copy(&p) })
}

macro_rules! api {
    ($name:ident, $($f:ident: fn($($a:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(dead_code)]
        pub struct $name { $(pub $f: unsafe extern "C" fn($($a),*) $(-> $r)?,)* }
        impl $name {
            pub fn load(h: *mut c_void) -> Result<$name, String> {
                unsafe { Ok($name { $($f: sym(h, stringify!($f))?,)* }) }
            }
        }
    };
}

// ---------------------------------------------------------------- mpv (client.h, render.h, render_gl.h)
#[repr(C)]
pub struct mpv_event {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct mpv_event_property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct mpv_event_end_file {
    pub reason: c_int,
    pub error: c_int,
}
#[repr(C)]
pub struct mpv_render_param {
    pub kind: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct mpv_opengl_init_params {
    pub get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub get_proc_address_ctx: *mut c_void,
}
#[repr(C)]
pub struct mpv_opengl_fbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

pub const EV_NONE: c_int = 0;
pub const EV_SHUTDOWN: c_int = 1;
pub const EV_START_FILE: c_int = 6;
pub const EV_END_FILE: c_int = 7;
pub const EV_FILE_LOADED: c_int = 8;
pub const EV_VIDEO_RECONFIG: c_int = 17;
pub const EV_PLAYBACK_RESTART: c_int = 21;
pub const EV_PROPERTY_CHANGE: c_int = 22;
pub const END_EOF: c_int = 0;
pub const END_ERROR: c_int = 4;
pub const FMT_STRING: c_int = 1;
pub const FMT_FLAG: c_int = 3;
pub const FMT_INT64: c_int = 4;
pub const FMT_DOUBLE: c_int = 5;
pub const RP_API_TYPE: c_int = 1;
pub const RP_OPENGL_INIT_PARAMS: c_int = 2;
pub const RP_OPENGL_FBO: c_int = 3;
pub const RP_FLIP_Y: c_int = 4;
pub const RP_ADVANCED_CONTROL: c_int = 10;
pub const UPDATE_FRAME: u64 = 1;

api!(Mpv,
    mpv_create: fn() -> Handle;
    mpv_initialize: fn(Handle) -> c_int;
    mpv_terminate_destroy: fn(Handle);
    mpv_set_option_string: fn(Handle, *const c_char, *const c_char) -> c_int;
    mpv_set_property_string: fn(Handle, *const c_char, *const c_char) -> c_int;
    mpv_get_property: fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
    mpv_get_property_string: fn(Handle, *const c_char) -> *mut c_char;
    mpv_set_property: fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
    mpv_command: fn(Handle, *mut *const c_char) -> c_int;
    mpv_command_async: fn(Handle, u64, *mut *const c_char) -> c_int;
    mpv_observe_property: fn(Handle, u64, *const c_char, c_int) -> c_int;
    mpv_wait_event: fn(Handle, f64) -> *mut mpv_event;
    mpv_set_wakeup_callback: fn(Handle, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void);
    mpv_free: fn(*mut c_void);
    mpv_error_string: fn(c_int) -> *const c_char;
    mpv_render_context_create: fn(*mut Handle, Handle, *mut mpv_render_param) -> c_int;
    mpv_render_context_set_update_callback: fn(Handle, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void);
    mpv_render_context_update: fn(Handle) -> u64;
    mpv_render_context_render: fn(Handle, *mut mpv_render_param) -> c_int;
    mpv_render_context_report_swap: fn(Handle);
    mpv_render_context_free: fn(Handle);
);

// ---------------------------------------------------------------- EGL 1.4 + wayland-egl
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLSurface = *mut c_void;
pub const EGL_NONE: i32 = 0x3038;
pub const EGL_RED_SIZE: i32 = 0x3024;
pub const EGL_GREEN_SIZE: i32 = 0x3023;
pub const EGL_BLUE_SIZE: i32 = 0x3022;
pub const EGL_ALPHA_SIZE: i32 = 0x3021;
pub const EGL_SURFACE_TYPE: i32 = 0x3033;
pub const EGL_WINDOW_BIT: i32 = 0x0004;
pub const EGL_RENDERABLE_TYPE: i32 = 0x3040;
pub const EGL_OPENGL_BIT: i32 = 0x0008;
pub const EGL_OPENGL_API: u32 = 0x30A2;
pub const EGL_CONTEXT_MAJOR_VERSION: i32 = 0x3098;
pub const EGL_CONTEXT_MINOR_VERSION: i32 = 0x30FB;
pub const EGL_CONTEXT_OPENGL_PROFILE_MASK: i32 = 0x30FD;
pub const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT: i32 = 0x1;

api!(Egl,
    eglGetDisplay: fn(*mut c_void) -> EGLDisplay;
    eglInitialize: fn(EGLDisplay, *mut i32, *mut i32) -> u32;
    eglBindAPI: fn(u32) -> u32;
    eglChooseConfig: fn(EGLDisplay, *const i32, *mut EGLConfig, i32, *mut i32) -> u32;
    eglCreateContext: fn(EGLDisplay, EGLConfig, EGLContext, *const i32) -> EGLContext;
    eglCreateWindowSurface: fn(EGLDisplay, EGLConfig, *mut c_void, *const i32) -> EGLSurface;
    eglDestroySurface: fn(EGLDisplay, EGLSurface) -> u32;
    eglMakeCurrent: fn(EGLDisplay, EGLSurface, EGLSurface, EGLContext) -> u32;
    eglSwapBuffers: fn(EGLDisplay, EGLSurface) -> u32;
    eglSwapInterval: fn(EGLDisplay, i32) -> u32;
    eglGetProcAddress: fn(*const c_char) -> *mut c_void;
    eglDestroyContext: fn(EGLDisplay, EGLContext) -> u32;
    eglTerminate: fn(EGLDisplay) -> u32;
);
api!(WlEgl,
    wl_egl_window_create: fn(*mut c_void, c_int, c_int) -> *mut c_void;
    wl_egl_window_resize: fn(*mut c_void, c_int, c_int, c_int, c_int);
    wl_egl_window_destroy: fn(*mut c_void);
);

pub struct Libs {
    pub mpv: Mpv,
    pub egl: Egl,
    pub wegl: WlEgl,
}
pub fn load() -> Result<Libs, String> {
    Ok(Libs { mpv: Mpv::load(lib("libmpv.so.2")?)?, egl: Egl::load(lib("libEGL.so.1")?)?, wegl: WlEgl::load(lib("libwayland-egl.so.1")?)? })
}

pub fn cstr(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}
