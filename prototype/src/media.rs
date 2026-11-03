//! Isolated libmpv host. Uses a child HWND, not a WebView or CPU frame uploads.
//! The DLL is optional and must be selected explicitly by absolute path.

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString, c_char, c_int, c_void},
    io::Write,
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{Bounds, Pixels, Point, Window};
use libloading::Library;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::HWND,
        System::SystemServices::SS_NOTIFY,
        UI::Input::KeyboardAndMouse::EnableWindow,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GA_ROOT, GWL_STYLE, GetAncestor, GetForegroundWindow,
            GetWindowLongPtrW, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
            WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
    core::w,
};

type Create = unsafe extern "C" fn() -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type SetOption = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type Command = unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> c_int;
type Observe = unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int;

type RequestLog = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

type Destroy = unsafe extern "C" fn(*mut c_void);
type WaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const Event;

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *const c_void,
}

#[repr(C)]
struct Property {
    name: *const c_char,
    format: c_int,
    data: *const c_void,
}

#[path = "media/diagnostics.rs"]
mod diagnostics;

#[path = "media/input.rs"]
mod input;

#[repr(C)]
struct LogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

pub(super) struct Player {
    handle: NonNull<c_void>,
    child: HWND,
    command: Command,
    destroy: Destroy,
    wait_event: WaitEvent,
    surface_bounds: Cell<Option<[i32; 4]>>,
    diagnostics: RefCell<diagnostics::Diagnostics>,
    last_sample: Cell<Instant>,
    failed: Cell<bool>,
    pointer_down: Cell<bool>,
    motion_gate: RefCell<input::MotionGate>,
    pointer_inside: Cell<bool>,
    fullscreen_requested: Cell<bool>,
    fullscreen_armed: Cell<bool>,
    panel_visible: Cell<bool>,
    _panel_script: tempfile::NamedTempFile,
    _library: Library,
    _thread_bound: PhantomData<Rc<()>>,
}

impl Player {
    pub(super) fn open(dll: &Path, media: &Path, window: &Window) -> Result<Self, String> {
        if !dll.is_absolute() || !dll.is_file() {
            return Err("Select an existing absolute libmpv DLL path first.".into());
        }
        let source = media
            .canonicalize()
            .map_err(|_| "Selected media is unavailable.")?;
        if !source.is_file() {
            return Err("Selected media is not a regular file.".into());
        }
        let source = source
            .to_str()
            .ok_or("Media path cannot be represented as UTF-8.")?;
        let source = source
            .strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .unwrap_or_else(|| source.strip_prefix(r"\\?\").unwrap_or(source).to_owned());
        let raw = HasWindowHandle::window_handle(window)
            .map_err(|error| error.to_string())?
            .as_raw();
        let RawWindowHandle::Win32(parent) = raw else {
            return Err("The prototype media host requires Windows.".into());
        };
        let parent = HWND(parent.hwnd.get() as *mut c_void);
        // SAFETY: the GPUI HWND belongs to this thread. Preserve all existing
        // styles and exclude native children from the parent's drawing region.
        let style = unsafe { GetWindowLongPtrW(parent, GWL_STYLE) };
        if style == 0 {
            return Err("Could not inspect native playback parent styles.".into());
        }

        if style & WS_CLIPCHILDREN.0 as isize == 0 {
            // SAFETY: change only the clipping flag on the live parent HWND.
            if unsafe { SetWindowLongPtrW(parent, GWL_STYLE, style | WS_CLIPCHILDREN.0 as isize) }
                == 0
            {
                return Err("Could not enable native playback child clipping.".into());
            }
        }

        // SAFETY: caller explicitly selects a native DLL; all symbols retain its lifetime.
        let library = unsafe { Library::new(dll) }
            .map_err(|_| "Could not load the selected libmpv DLL or its dependencies.")?;
        // SAFETY: symbol signatures follow the stable libmpv client ABI.
        let (create, initialize, set_option, command, destroy, wait_event, observe) = unsafe {
            (
                *library
                    .get::<Create>(b"mpv_create\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<Initialize>(b"mpv_initialize\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<SetOption>(b"mpv_set_option_string\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<Command>(b"mpv_command_async\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<Destroy>(b"mpv_terminate_destroy\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<WaitEvent>(b"mpv_wait_event\0")
                    .map_err(symbol_error)?,
                *library
                    .get::<Observe>(b"mpv_observe_property\0")
                    .map_err(symbol_error)?,
            )
        };
        let mut panel_script = tempfile::Builder::new()
            .prefix("nexa-gpui-panel-")
            .suffix(".lua")
            .tempfile()
            .map_err(|_| "Could not prepare embedded playback panel.")?;
        panel_script
            .write_all(include_bytes!("media/panel.lua"))
            .map_err(|_| "Could not write embedded playback panel.")?;
        // SAFETY: stable client ABI; library outlives this call and player.
        let request_log = unsafe {
            *library
                .get::<RequestLog>(b"mpv_request_log_messages\0")
                .map_err(symbol_error)?
        };
        // SAFETY: standard STATIC class, current UI thread, valid parent from GPUI.
        let child = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                // STATIC without SS_NOTIFY returns HTTRANSPARENT on hit testing,
                // allowing the parent UI to win instead of the embedded surface.
                WS_CHILD
                    | WS_VISIBLE
                    | WS_CLIPSIBLINGS
                    | WS_CLIPCHILDREN
                    | WINDOW_STYLE(SS_NOTIFY.0),
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                None,
                None,
            )
        }
        .map_err(|_| "Could not create native playback surface.")?;
        // SAFETY: owned live child. Its descendants render normally but receive no
        // native input; GPUI is the sole pointer owner and forwards commands.
        let _ = unsafe { EnableWindow(child, false) };
        tracing::info!(
            operation = "playback_surface",
            input_route = "gpui_commands",
            "Created render-only native video container"
        );
        // SAFETY: function pointer is owned by the loaded library.
        let Some(handle) = NonNull::new(unsafe { create() }) else {
            // SAFETY: this child was created on this thread and has no owner yet.
            let _ = unsafe { DestroyWindow(child) };
            return Err("Could not create libmpv player.".into());
        };
        let player = Self {
            handle,
            child,
            command,
            destroy,
            wait_event,
            surface_bounds: Cell::new(None),
            diagnostics: RefCell::new(diagnostics::Diagnostics::default()),
            last_sample: Cell::new(Instant::now()),
            failed: Cell::new(false),
            pointer_down: Cell::new(false),
            motion_gate: RefCell::new(input::MotionGate::default()),
            pointer_inside: Cell::new(false),
            fullscreen_requested: Cell::new(false),
            fullscreen_armed: Cell::new(false),
            panel_visible: Cell::new(true),
            _panel_script: panel_script,
            _library: library,
            _thread_bound: PhantomData,
        };
        for (key, value) in [
            // libmpv specifies a nonnegative uint32_t Windows handle value.
            ("wid", (child.0 as usize as u32).to_string()),
            ("config", "no".into()),
            ("load-scripts", "no".into()),
            ("osc", "no".into()),
            ("input-cursor", "yes".into()),
            // Permit script bindings and native VO input for the embedded OSC.
            // Suppress stock shortcuts separately; fullscreen belongs to GPUI.
            ("input-default-bindings", "yes".into()),
            ("input-builtin-bindings", "no".into()),
            ("input-vo-keyboard", "no".into()),
            ("vo", "gpu".into()),
            ("gpu-context", "d3d11".into()),
            ("hwdec", "auto-safe".into()),
            ("keep-open", "yes".into()),
        ] {
            let key = CString::new(key).map_err(|_| "Invalid option name.")?;
            let value = CString::new(value).map_err(|_| "Invalid option value.")?;
            // SAFETY: handle and NUL-terminated arguments stay live for the call.
            if unsafe { set_option(handle.as_ptr(), key.as_ptr(), value.as_ptr()) } < 0 {
                return Err(
                    "The selected libmpv build rejected a required playback option.".into(),
                );
            }
        }
        // SAFETY: options are set and this player exclusively owns its handle.
        if unsafe { initialize(handle.as_ptr()) } < 0 {
            return Err("Could not initialize libmpv's Direct3D playback path.".into());
        }
        // SAFETY: initialized handle and static NUL-terminated level.
        if unsafe { request_log(handle.as_ptr(), c"info".as_ptr()) } < 0 {
            tracing::warn!(
                operation = "playback_panel",
                "Panel engine log observation unavailable"
            );
        }
        for (index, name) in diagnostics::PROPERTIES.iter().enumerate() {
            let name = CString::new(*name).map_err(|_| "Invalid diagnostic property.")?;
            // SAFETY: live handle and string; event payload is consumed before next poll.
            if unsafe { observe(handle.as_ptr(), index as u64 + 1, name.as_ptr(), 1) } < 0 {
                tracing::warn!(
                    property = diagnostics::PROPERTIES[index],
                    "Metric observation unavailable"
                );
            }
        }
        tracing::info!(
            operation = "playback_input",
            script_bindings = true,
            native_vo_input = false,
            stock_bindings = false,
            "OSC script input enabled; host retains external controls"
        );
        let panel_path = player
            ._panel_script
            .path()
            .to_str()
            .ok_or("Panel path is not UTF-8.")?;
        player.send(&["load-script", panel_path])?;
        tracing::info!(
            operation = "playback_panel",
            "Embedded Nexa playback panel requested"
        );
        player.send(&["loadfile", &source, "replace"])?;
        tracing::info!(
            renderer = "d3d11",
            hwdec = "auto-safe",
            "Native playback requested; actual decoder unverified"
        );
        Ok(player)
    }

    pub(super) fn send(&self, arguments: &[&str]) -> Result<(), String> {
        let command_name = arguments.first().copied().unwrap_or("unknown");
        let arguments = arguments
            .iter()
            .map(|value| CString::new(*value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Playback argument contains a NUL.")?;
        let mut pointers: Vec<_> = arguments.iter().map(|value| value.as_ptr()).collect();
        pointers.push(std::ptr::null());
        // SAFETY: libmpv copies command arguments before returning; null terminates argv.
        let request_id = match command_name {
            "mouse" => 1,
            "keydown" => 2,
            "keyup" => 3,
            "load-script" => 4,
            "loadfile" => 5,
            "script-message" => 6,
            _ => 7,
        };
        let result = unsafe { (self.command)(self.handle.as_ptr(), request_id, pointers.as_ptr()) };
        if result < 0 {
            return Err(format!(
                "Playback command {command_name} was rejected ({result})."
            ));
        }
        Ok(())
    }

    pub(super) fn pointer(
        &self,
        position: Point<Pixels>,
        scale: f32,
        down: Option<bool>,
    ) -> Result<(), String> {
        let Some(bounds) = self.surface_bounds.get() else {
            return Ok(());
        };
        let local = input::local_position(
            bounds,
            [f32::from(position.x), f32::from(position.y)],
            scale,
        );
        let inside = local.is_some();
        if let Some([x, y]) = local {
            if self.motion_gate.borrow_mut().admit(
                Instant::now(),
                [x, y],
                down == Some(true) || (down == Some(false) && self.pointer_down.get()),
            ) {
                self.send(&["mouse", &x.to_string(), &y.to_string()])?;
            }
        } else if self.pointer_inside.get() {
            self.send(&["mouse", "-1", "-1"])?;
        }
        if inside != self.pointer_inside.replace(inside) {
            tracing::info!(
                operation = "video_pointer",
                inside,
                "Video pointer boundary crossed"
            );
        }
        if !inside {
            self.release_pointer()?;
        }

        let fullscreen_hit = local.is_some_and(|[x, y]| {
            let vx = x as f32 * 1000.0 / bounds[2] as f32;
            let bottom = (bounds[3] - y) as f32 * 1000.0 / bounds[2] as f32;
            (920.0..980.0).contains(&vx) && (17.0..71.0).contains(&bottom)
        });
        match down {
            Some(true) if inside && !self.pointer_down.get() => {
                self.send(&["keydown", "MBTN_LEFT"])?;
                self.pointer_down.set(true);
                self.fullscreen_armed
                    .set(fullscreen_hit && self.panel_visible.get());
                tracing::info!(
                    operation = "video_pointer",
                    phase = "down",
                    "OSC pointer forwarded"
                );
            }
            Some(false) => {
                if self.pointer_down.get()
                    && self.fullscreen_armed.get()
                    && fullscreen_hit
                    && self.panel_visible.get()
                {
                    self.fullscreen_requested.set(true);
                }
                self.release_pointer()?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn set_panel_visible(&self, visible: bool) -> Result<(), String> {
        self.send(&[
            "script-message",
            "nexa-panel-visibility",
            if visible { "always" } else { "never" },
        ])?;
        self.release_pointer()?;
        self.panel_visible.set(visible);
        Ok(())
    }

    pub(super) fn take_fullscreen_request(&self) -> bool {
        self.fullscreen_requested.replace(false)
    }

    fn release_pointer(&self) -> Result<(), String> {
        if self.pointer_down.get() {
            self.send(&["keyup", "MBTN_LEFT"])?;
            self.pointer_down.set(false);
            self.fullscreen_armed.set(false);
            tracing::info!(
                operation = "video_pointer",
                phase = "release",
                "OSC pointer released"
            );
        }
        Ok(())
    }

    pub(super) fn resize(&self, bounds: Bounds<Pixels>, scale: f32) {
        let x = (f32::from(bounds.origin.x) * scale).round() as i32;
        let y = (f32::from(bounds.origin.y) * scale).round() as i32;
        let width = (f32::from(bounds.size.width) * scale).round().max(1.0) as i32;
        let height = (f32::from(bounds.size.height) * scale).round().max(1.0) as i32;
        let geometry = [x, y, width, height];
        if self.surface_bounds.get() == Some(geometry) {
            return;
        }

        // SAFETY: called during paint on the owning UI thread; child remains alive.
        match unsafe {
            SetWindowPos(
                self.child,
                None,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        } {
            Ok(()) => {
                if self.surface_bounds.replace(Some(geometry)).is_none() {
                    tracing::info!(
                        x,
                        y,
                        width,
                        height,
                        scale,
                        "Native playback surface positioned"
                    );
                } else {
                    tracing::debug!(
                        x,
                        y,
                        width,
                        height,
                        scale,
                        "Native playback surface resized"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    hresult = error.code().0,
                    "Could not position native playback surface"
                );
            }
        }
    }

    pub(super) fn poll(&self) -> Option<Result<String, String>> {
        // SAFETY: live owned child; focus reads do not send messages or block.
        let active = unsafe { GetForegroundWindow() == GetAncestor(self.child, GA_ROOT) };
        if !active && let Err(error) = self.release_pointer() {
            return Some(Err(error));
        }
        let mut outcome: Option<Result<String, String>> = None;
        for _ in 0..32 {
            // SAFETY: nonblocking poll; event is consumed before the next poll invalidates it.
            let event = unsafe { (self.wait_event)(self.handle.as_ptr(), 0.0).as_ref() }?;
            if event.id == 0 {
                break;
            }
            if event.id == 2 && !event.data.is_null() {
                // SAFETY: LOG_MESSAGE payload is valid until next wait_event.
                let log = unsafe { &*event.data.cast::<LogMessage>() };
                if !log.prefix.is_null() && !log.level.is_null() {
                    let prefix = unsafe { CStr::from_ptr(log.prefix) }.to_string_lossy();
                    let level = unsafe { CStr::from_ptr(log.level) }.to_string_lossy();
                    if prefix.starts_with("nexa-gpui-panel-")
                        || prefix.starts_with("nexa_gpui_panel_")
                    {
                        if level == "error" || level == "fatal" {
                            outcome = Some(Err("Custom playback panel failed in libmpv; external controls remain available.".into()));
                        } else if level == "info" {
                            tracing::info!(
                                operation = "playback_panel",
                                "Custom panel script reports initialization"
                            );
                        }
                    }
                }
            } else if event.id == 22 && !event.data.is_null() {
                // SAFETY: PROPERTY_CHANGE provides Property; STRING data is char**.
                let property = unsafe { &*event.data.cast::<Property>() };
                let value = if property.format == 1 && !property.data.is_null() {
                    let pointer = unsafe { *property.data.cast::<*const c_char>() };
                    if pointer.is_null() {
                        None
                    } else {
                        Some(
                            unsafe { CStr::from_ptr(pointer) }
                                .to_string_lossy()
                                .into_owned(),
                        )
                    }
                } else {
                    None
                };
                self.diagnostics
                    .borrow_mut()
                    .update(event.reply_userdata, value);
            } else if event.id == 24 {
                self.diagnostics.borrow_mut().clear();
                tracing::warn!("Playback event queue overflow; diagnostics invalidated");
            } else if event.error < 0 {
                let command_name = match event.reply_userdata {
                    1 => "mouse",
                    2 => "keydown",
                    3 => "keyup",
                    4 => "load-script",
                    5 => "loadfile",
                    6 => "script-message",
                    _ => "other",
                };
                outcome = Some(Err(if event.id == 5 {
                    format!(
                        "Playback command {command_name} failed asynchronously ({}).",
                        event.error
                    )
                } else {
                    format!("Native playback failed ({}).", event.error)
                }));
            } else if event.id == 8 {
                outcome = Some(Ok("Video loaded - awaiting decoder telemetry".into()));
                tracing::info!("Native media loaded");
            } else if event.id == 7 && !event.data.is_null() {
                // SAFETY: MPV_EVENT_END_FILE guarantees the documented EndFile payload.
                let end = unsafe { &*event.data.cast::<EndFile>() };
                if end.error < 0 {
                    outcome = Some(Err(format!(
                        "Media loading or playback failed ({}).",
                        end.error
                    )));
                }
            }
        }
        if matches!(&outcome, Some(Err(_))) {
            self.failed.set(true);
        }

        if self.last_sample.get().elapsed() >= Duration::from_secs(1) {
            self.last_sample.set(Instant::now());
            let summary = self.diagnostics.borrow().summary();
            tracing::info!(operation = "playback_sample", metrics = %summary, "Observed libmpv telemetry");
            if outcome.is_none() && !self.failed.get() {
                outcome = Some(Ok(summary));
            }
        }
        outcome
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: decoder stops before its HWND and library are released; UI-thread-bound owner.
        unsafe {
            (self.destroy)(self.handle.as_ptr());
            let _ = DestroyWindow(self.child);
        }
        tracing::info!("Native player released");
    }
}

fn symbol_error(_: libloading::Error) -> String {
    "The selected DLL does not expose the required libmpv client API.".into()
}
