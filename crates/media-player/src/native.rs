use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
unsafe extern "C" {
    fn vsf_surface_create(parent: *mut c_void) -> *mut c_void;
    fn vsf_surface_layout(
        surface: *mut c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        visible: c_int,
    );
    fn vsf_surface_release(surface: *mut c_void);
    #[cfg(any(
        target_os = "windows",
        target_os = "linux",
        all(target_os = "macos", feature = "macos-angle")
    ))]
    fn vsf_surface_occlude(surface: *mut c_void, occluded: c_int) -> c_int;
    fn vsf_player_create(surface: *mut c_void, error: *mut c_int) -> *mut c_void;
    fn vsf_player_destroy(player: *mut c_void);
    fn vsf_command(player: *mut c_void, args: *const *const c_char) -> c_int;
    fn vsf_discard_events(player: *mut c_void) -> c_int;
    fn vsf_presenting(player: *mut c_void, value: c_int);
    fn vsf_number(player: *mut c_void, name: *const c_char) -> f64;
    fn vsf_flag(player: *mut c_void, name: *const c_char) -> c_int;
    fn vsf_frames(player: *mut c_void) -> u64;
    fn vsf_event_error(player: *mut c_void, file_transition: *mut c_int) -> c_int;
    fn vsf_text(player: *mut c_void, name: *const c_char) -> *mut c_char;
    fn vsf_text_free(text: *mut c_char);
    fn vsf_error(code: c_int) -> *const c_char;
}

pub struct Surface(Arc<SurfaceInner>);
struct SurfaceInner {
    raw: *mut c_void,
    active: AtomicBool,
}
// SAFETY: native surface retains its child; non-main operations only copy
// layout state or dispatch native window access onto the UI thread. Rust owns
// the native reference; Player retains this Arc until core teardown completes.
unsafe impl Send for SurfaceInner {}
// SAFETY: only the active Player can use the native context; AtomicBool grants
// that lease. Native layout/removal never mutates the view/window outside the UI thread. This inner type is private.
unsafe impl Sync for SurfaceInner {}
impl Surface {
    /// Acknowledge hiding an above-webview surface before an HTML overlay opens.
    /// Called on the control worker, never the UI thread. Only the legacy Mac
    /// OpenGL path draws below its webview and needs no explicit occlusion.
    pub fn occlude(&self, occluded: bool) -> Result<(), String> {
        #[cfg(any(
            target_os = "windows",
            target_os = "linux",
            all(target_os = "macos", feature = "macos-angle")
        ))]
        {
            // SAFETY: this Arc retains the surface. Native code releases its
            // state lock before waiting for the live UI thread to apply hiding.
            if unsafe { vsf_surface_occlude(self.0.raw, occluded.into()) } == 0 {
                return Err("Could not update native video occlusion".into());
            }
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "linux",
            all(target_os = "macos", feature = "macos-angle")
        )))]
        let _ = occluded;
        Ok(())
    }

    /// Queue logical web-content geometry; native window changes run on main.
    pub fn layout(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        visible: bool,
    ) -> Result<(), String> {
        if ![x, y, width, height]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 32768.0)
            || width <= 0.0
            || height <= 0.0
        {
            return Err("Invalid native surface bounds".into());
        }
        // SAFETY: live owner; native cleanup acknowledges UI removal before freeing queued-call state.
        unsafe { vsf_surface_layout(self.0.raw, x, y, width, height, visible.into()) };
        Ok(())
    }

    /// Attach the native experiment to a live platform parent.
    ///
    /// # Safety
    /// `parent` must be a live NSView on macOS, HWND on Windows, or Tauri default GtkBox on Linux. The caller
    /// must be on the UI thread and retain the parent until this surface drops.
    pub unsafe fn attach(parent: *mut c_void) -> Result<Self, String> {
        // SAFETY: caller establishes the platform pointer and UI-thread requirements.
        let raw = unsafe { vsf_surface_create(parent) };
        if raw.is_null() {
            Err("Could not attach native video surface".into())
        } else {
            Ok(Self(Arc::new(SurfaceInner {
                raw,
                active: AtomicBool::new(false),
            })))
        }
    }
}
impl Drop for SurfaceInner {
    fn drop(&mut self) {
        // SAFETY: sole owner, released once; native removal is acknowledged on the UI thread.
        unsafe { vsf_surface_release(self.raw) }
    }
}

/// Created, commanded, and dropped on one control worker; intentionally !Send/!Sync.
/// Native code stops rendering before destroying mpv; Windows uses mpv
/// internal D3D11 threads, macOS owns a separate render thread, and Linux
/// dispatches render-API work to GTK main.
pub struct Player {
    raw: *mut c_void,
    _thread: PhantomData<Rc<()>>,
    surface: Arc<SurfaceInner>,
    file_loaded: bool,
}
impl Player {
    /// Unload the file while retaining the core/render context. Only the control
    /// worker may wait here. An error requires dropping this engine before reuse.
    pub fn unload(&mut self) -> Result<(), String> {
        self.command(&["stop"])?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            // SAFETY: live worker-owned handle; property read does not access AppKit.
            if unsafe { vsf_flag(self.raw, c"idle-active".as_ptr()) } != 0 {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("Timed out unloading media".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // SAFETY: file is idle; this worker is the sole client-event consumer.
        let result = unsafe { vsf_discard_events(self.raw) };
        if result < 0 {
            return Err(error(result));
        }
        self.presenting(false);
        for (name, value) in [
            ("pause", "yes"),
            ("ab-loop-a", "no"),
            ("ab-loop-b", "no"),
            ("aid", "auto"),
        ] {
            self.command(&["set", name, value])?;
        }
        Ok(())
    }

    pub fn presenting(&mut self, value: bool) {
        // SAFETY: live owner; native code only changes synchronized presentation state.
        unsafe { vsf_presenting(self.raw, value.into()) };
    }

    pub fn new(surface: &Surface) -> Result<Self, String> {
        if surface.0.active.swap(true, Ordering::AcqRel) {
            return Err("Surface already has an active player".into());
        }
        let mut code = 0;
        // SAFETY: surface owns a valid retained native reference. Native player
        // retains or borrows the native surface, kept alive by the Arc below.
        let raw = unsafe { vsf_player_create(surface.0.raw, &mut code) };
        if raw.is_null() {
            surface.0.active.store(false, Ordering::Release);
            Err(error(code))
        } else {
            Ok(Self {
                raw,
                _thread: PhantomData,
                surface: surface.0.clone(),
                file_loaded: false,
            })
        }
    }
    pub fn command(&mut self, args: &[&str]) -> Result<(), String> {
        let strings: Vec<_> = args
            .iter()
            .map(|s| CString::new(*s))
            .collect::<Result<_, _>>()
            .map_err(|_| "Command contains a NUL byte")?;
        let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        // SAFETY: null-terminated array and strings remain live during synchronous call.
        let result = unsafe { vsf_command(self.raw, pointers.as_ptr()) };
        if result < 0 {
            Err(error(result))
        } else {
            if matches!(args.first(), Some(&"loadfile" | &"stop")) {
                self.file_loaded = false;
            }
            Ok(())
        }
    }
    pub fn number(&mut self, name: &CStr) -> Option<f64> {
        self.signed_number(name).filter(|value| *value >= 0.0)
    }
    pub fn signed_number(&mut self, name: &CStr) -> Option<f64> {
        // SAFETY: handle is live and accessed only by its owner; name is NUL-terminated.
        let value = unsafe { vsf_number(self.raw, name.as_ptr()) };
        value.is_finite().then_some(value)
    }
    pub fn text(&mut self, name: &CStr) -> Option<String> {
        // SAFETY: live worker-owned handle and static NUL-terminated property.
        let raw = unsafe { vsf_text(self.raw, name.as_ptr()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: mpv owns this NUL-terminated allocation until explicitly freed.
        let value = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: release this successful property allocation exactly once.
        unsafe { vsf_text_free(raw) };
        Some(value)
    }
    pub fn paused(&mut self) -> bool {
        self.flag(c"pause")
    }
    pub fn flag(&mut self, name: &CStr) -> bool {
        // SAFETY: valid handle; native function writes its own local flag.
        unsafe { vsf_flag(self.raw, name.as_ptr()) != 0 }
    }
    pub fn seeking(&mut self) -> bool {
        self.flag(c"seeking")
    }
    pub fn frames(&self) -> u64 {
        // SAFETY: native counter is atomic; handle is live.
        unsafe { vsf_frames(self.raw) }
    }
    pub fn event_error(&mut self) -> Option<String> {
        let mut transition = 0;
        // SAFETY: only control owner polls this core's event queue.
        // Native code writes the last file transition in this bounded batch;
        // it retains neither the pointer nor any borrowed event data.
        let code = unsafe { vsf_event_error(self.raw, &mut transition) };
        if transition != 0 {
            self.file_loaded = transition > 0;
        }
        (code < 0).then(|| error(code))
    }
    /// FILE_LOADED has completed track initialization for the current load.
    pub fn file_loaded(&self) -> bool {
        self.file_loaded
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: sole owner; native code joins rendering before core destruction.
        unsafe { vsf_player_destroy(self.raw) }
        self.surface.active.store(false, Ordering::Release);
    }
}
fn error(code: c_int) -> String {
    // SAFETY: mpv returns a static NUL-terminated error string for any integer code.
    unsafe { CStr::from_ptr(vsf_error(code)) }
        .to_string_lossy()
        .into_owned()
}
