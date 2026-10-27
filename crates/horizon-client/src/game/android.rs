//! The game on Android (`android_main.rs` starts it after the launcher): the
//! touch screen, the soft keyboard, a gamepad, the gyroscope, the back
//! gesture and the window that goes away while the app is in the background.
//!
//! Touch works as in Zalith Launcher 2. The control layout
//! (`horizon_client::controls`) takes the fingers that land on its buttons
//! and joysticks, and its floating ball opens the game menu and the layout
//! editor. In the world any other finger looks around (with gestures on, a
//! tap uses and a hold attacks), and the hotbar takes taps (select), double
//! taps (swap hands) and long presses (drop). Over a screen a finger moves a
//! mouse pointer the way a touchpad does: a tap clicks where it points and a
//! long press holds the button.
use super::*;
use crate::android::jni::{self, Typed};
use horizon_client::controls::editor::{Editor, Outcome};
use horizon_client::controls::menu::{Choice, Menu, Settings as TouchSettings};
use horizon_client::controls::{self, Claim, Controls, Event as ControlEvent, Screen as ControlScreen};
use horizon_client::gamepad::{Button as PadButton, PadState};
use horizon_client::osk;
use horizon_client::ui_theme::{preference, set_preference};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use winit::event::{Touch as Finger, TouchPhase};
use winit::keyboard::NativeKeyCode;

/// Zalith's long press and double tap timeouts.
const LONG_PRESS: Duration = Duration::from_millis(300);
const DOUBLE_TAP: Duration = Duration::from_millis(300);
/// How far a finger moves before a tap is a drag, in dp.
const SLOP: f32 = 8.0;
/// Mouse counts per radian the phone turns with the gyroscope on.
const GYRO: f32 = 380.0;

/// What the HUD draws for touch, kept for `draw_touch`.
struct Overlay {
    controls: Controls,
    menu: Menu,
    editor: Option<Editor>,
    settings: TouchSettings,
    screen: ControlScreen,
    in_game: bool,
    /// The menu's mouse pointer, physical pixels, while a screen is open.
    pointer: Option<(f32, f32)>,
    shown: bool,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
    /// The gamepad as the activity's events left it.
    static GAMEPAD: Cell<PadState> = Cell::new(PadState::default());
    /// Where text at the screen's edges stays clear of, physical pixels:
    /// left, top, right, bottom.
    static HUD_MARGINS: Cell<[f32; 4]> = const { Cell::new([0.0; 4]) };
}

/// The margins the debug screen keeps from the edges (`game/debug_keys.rs`).
pub(crate) fn hud_margins() -> [f32; 4] {
    HUD_MARGINS.with(Cell::get)
}

/// The gamepad, for the game's controller support (`game/pad.rs`).
pub(crate) fn gamepad() -> PadState {
    GAMEPAD.with(Cell::get)
}

/// The touch controls, the menu ball and the menu's pointer over the HUD.
pub(crate) fn draw_touch(batch: &mut HudBatch, atlas: &horizon_client::hud::HudAtlas) {
    OVERLAY.with(|o| {
        let Some(o) = &mut *o.borrow_mut() else {
            return;
        };
        if !o.shown {
            return;
        }
        if let Some(editor) = &mut o.editor {
            editor.draw(batch, atlas, o.screen);
            return;
        }
        if !o.settings.controls_hidden {
            o.controls.draw(batch, atlas, o.in_game, o.screen, o.settings.opacity);
        }
        o.menu.draw(batch, atlas, o.screen, o.settings);
        if let Some(at) = o.pointer {
            controls::draw_pointer(batch, atlas, at, 24.0 * o.screen.density);
        }
    });
}

fn with_overlay<R>(f: impl FnOnce(&mut Overlay) -> R) -> Option<R> {
    OVERLAY.with(|o| o.borrow_mut().as_mut().map(f))
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// A control, the menu or the editor has it.
    Control,
    /// Looking around in the world; `held` is the button a gesture's hold pressed.
    Look { gestures: bool, held: Option<MouseButton> },
    /// Moving the menu's pointer; `held` once a long press pressed the button.
    Pointer { move_only: bool, held: bool },
    /// On the hotbar, over `slot`.
    Hotbar { slot: usize, dropped: bool },
}

struct Contact {
    role: Role,
    start: (f32, f32),
    at: (f32, f32),
    since: Instant,
    moved: bool,
}

pub(crate) struct AndroidGame {
    app: App,
    home: PathBuf,
    contacts: HashMap<u64, Contact>,
    /// Mouse buttons the controls hold down.
    buttons: Vec<MouseButton>,
    /// Clicks a gesture tap made, released a moment later.
    releases: Vec<(MouseButton, Instant)>,
    /// A launcher scroll event held: the direction and the next wheel step.
    scrolling: Option<(f32, Instant)>,
    /// The last tap on the hotbar, for a double tap.
    last_slot_tap: Option<(usize, Instant)>,
    keyboard: bool,
    /// The IME button opened the keyboard without a field.
    forced_keyboard: bool,
    enabled: bool,
    /// Physical pixels per dp.
    dp: f32,
    dark: bool,
    was_in_game: bool,
}

/// The control layout the player picked (`controlLayout` in horizon.txt,
/// a file in `controls/`), else the default.
fn load_controls(home: &Path) -> Controls {
    let name = preference(home, "controlLayout").unwrap_or_default();
    if !name.is_empty() {
        let path = home.join("controls").join(format!("{name}.json"));
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|json| controls::layout::Layout::parse(&json))
        {
            Ok(layout) => return Controls::new(layout),
            Err(e) => log::warn!("Horizon: control layout {}: {e}", path.display()),
        }
    }
    Controls::default_layout()
}

fn flag(home: &Path, key: &str, default: bool) -> bool {
    preference(home, key).map_or(default, |v| v == "true")
}

fn set_flag(home: &Path, key: &str, on: bool) {
    set_preference(home, key, Some(if on { "true" } else { "false" }));
}

/// Writes an edited layout: over the file it came from, or to a new one when
/// it was the default. Returns the file's name.
fn save_layout(home: &Path, layout: &controls::layout::Layout) -> std::io::Result<String> {
    let dir = home.join("controls");
    std::fs::create_dir_all(&dir)?;
    let mut stem = preference(home, "controlLayout").unwrap_or_default();
    if stem.is_empty() {
        stem = (1..)
            .map(|n| format!("custom-{n}"))
            .find(|s| !dir.join(format!("{s}.json")).exists())
            .unwrap_or_default();
    }
    let mut layout = layout.clone();
    if layout.info.name.default == "Default" && stem.starts_with("custom-") {
        layout.info.name.default = format!("My layout {}", stem.trim_start_matches("custom-"));
    }
    std::fs::write(dir.join(format!("{stem}.json")), layout.to_json())?;
    set_preference(home, "controlLayout", Some(&stem));
    Ok(stem)
}

impl AndroidGame {
    /// The game with `args` (`--resources`, `--session-file`), at its loading screen.
    pub(crate) fn start(args: Vec<String>) -> Result<Self> {
        let options = Options::from_args(args.into_iter())?.ok_or("no options")?;
        horizon_threads::put_helpers_behind();
        horizon_threads::favour_current();
        horizon_client::heartbeat::start();
        let started = Instant::now();
        let vanilla_root = options.resources.clone();
        let home = std::env::current_dir()?;
        let settings = Settings::load(&home.join("options.txt"));
        Behaviours::set_cache_dir(home.join(".horizon-cache"));
        let enabled = flag(&home, "touchControls", true);
        let dp = jni::platform("density", "")
            .and_then(|d| d.parse().ok())
            .unwrap_or(2.0);
        let dark = jni::platform("dark", "").as_deref() == Some("1");
        // Roboto for the control labels, as Zalith's, and symbols Geist lacks.
        let font = |name: &str| std::fs::read(Path::new("/system/fonts").join(name)).ok();
        horizon_client::ui_theme::set_system_fonts(
            font("RobotoStatic-Regular.ttf").or_else(|| font("Roboto-Regular.ttf")),
            [
                "NotoSansSymbols-Regular-Subsetted.ttf",
                "NotoSansSymbols-Regular-Subsetted2.ttf",
                "NotoSansSymbols2-Regular.ttf",
            ]
            .iter()
            .filter_map(|name| font(name))
            .collect(),
        );
        let mut controls = load_controls(&home);
        controls.language = jni::platform("language", "").unwrap_or_default();
        log::info!(
            "Horizon: touch controls {}, layout {:?}",
            if enabled { "on" } else { "off (Settings, Touch controls)" },
            controls.layout.info.name.default.trim()
        );
        let touch = TouchSettings {
            gestures: flag(&home, "touchGestures", false),
            gyroscope: flag(&home, "touchGyroscope", false),
            opacity: preference(&home, "touchOpacity")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
            controls_hidden: false,
            size: preference(&home, "touchSize")
                .and_then(|v| v.parse().ok())
                .filter(|v: &f32| (0.3..=2.0).contains(v)),
            fitted: 1.0,
            larger_interface: flag(&home, "largerInterface", false),
            debug_scale: settings.file.int("debugGuiScale"),
        };
        // Text at the edges clears the camera's cutout and the screen's
        // rounded corners and curved sides.
        let insets: Vec<f32> = jni::platform("insets", "")
            .unwrap_or_default()
            .split(',')
            .map(|v| v.parse().unwrap_or(0.0))
            .collect();
        let inset = |i: usize| insets.get(i).copied().unwrap_or(0.0);
        HUD_MARGINS.with(|m| {
            m.set([
                inset(0).max(16.0 * dp),
                inset(1).max(6.0 * dp),
                inset(2).max(16.0 * dp),
                inset(3),
            ])
        });
        set_larger_interface(touch.larger_interface);
        let ball = preference(&home, "touchMenuBall")
            .and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some((x.parse().ok()?, y.parse().ok()?))
            })
            .unwrap_or((0.5, 0.05));
        if touch.gyroscope {
            jni::gyroscope(true);
        }
        OVERLAY.with(|o| {
            *o.borrow_mut() = Some(Overlay {
                controls,
                menu: Menu::new(ball),
                editor: None,
                settings: touch,
                screen: ControlScreen {
                    width: 1.0,
                    height: 1.0,
                    density: dp,
                    dark,
                },
                in_game: false,
                pointer: None,
                shown: false,
            })
        });
        Ok(Self {
            app: loading_app(options, settings, vanilla_root, None, started)?,
            home,
            contacts: HashMap::new(),
            buttons: Vec::new(),
            releases: Vec::new(),
            scrolling: None,
            last_slot_tap: None,
            keyboard: false,
            forced_keyboard: false,
            enabled,
            dp,
            dark,
            was_in_game: false,
        })
    }

    /// The game's error once it stopped the loop.
    pub(crate) fn error(&mut self) -> Option<Box<dyn std::error::Error>> {
        match &mut self.app {
            App::Ready(game) => {
                let _ = write_report(game);
                game.error.take()
            }
            App::Loading(loading) => loading.error.take(),
        }
    }

    fn window(&self) -> Option<&Arc<Window>> {
        match &self.app {
            App::Ready(game) => game.window.as_ref(),
            App::Loading(loading) => loading.window.as_ref(),
        }
    }

    fn renderer(&mut self) -> Option<&mut Renderer> {
        match &mut self.app {
            App::Ready(game) => game.renderer.as_mut(),
            App::Loading(loading) => loading.renderer.as_mut(),
        }
    }

    fn screen(&self) -> ControlScreen {
        let size = match &self.app {
            App::Ready(game) => game.renderer.as_ref().map(|r| [r.config.width, r.config.height]),
            App::Loading(loading) => loading
                .renderer
                .as_ref()
                .map(|r| [r.config.width, r.config.height]),
        }
        .or_else(|| self.window().map(|w| w.inner_size().into()))
        .unwrap_or([1, 1]);
        ControlScreen {
            width: size[0] as f32,
            height: size[1] as f32,
            density: self.dp,
            dark: self.dark,
        }
    }

    /// The world has the pointer, with nothing over it.
    fn in_game(&mut self) -> bool {
        match &mut self.app {
            App::Ready(game) => game.touch_playing(),
            App::Loading(_) => false,
        }
    }

    fn editing(&self) -> bool {
        with_overlay(|o| o.editor.is_some()).unwrap_or(false)
    }

    pub(crate) fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window().is_none() {
            self.app.resumed(event_loop);
            return;
        }
        // Back on screen: a new native window behind the same winit window.
        let size = self.window().map(|w| w.inner_size());
        if let Some(renderer) = self.renderer() {
            if let Err(e) = renderer.recreate_surface() {
                log::warn!("Horizon: the window came back without a surface: {e}");
            }
            if let Some(size) = size {
                renderer.resize(size.width, size.height);
            }
        }
        if let App::Ready(game) = &mut self.app {
            game.audio.set_output(true);
        }
        if with_overlay(|o| o.settings.gyroscope).unwrap_or(false) {
            jni::gyroscope(true);
        }
        if let Some(w) = self.window() {
            w.request_redraw();
        }
    }

    /// The app left the screen and its window with it.
    pub(crate) fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        jni::show_keyboard(false);
        jni::gyroscope(false);
        self.keyboard = false;
        self.forced_keyboard = false;
        if let Some(renderer) = self.renderer() {
            renderer.release_surface();
            renderer.suspended = true;
        }
        self.contacts.clear();
        GAMEPAD.with(|g| g.set(PadState::default()));
        with_overlay(|o| o.controls.release_all());
        self.apply_control_events(event_loop);
        self.release_mouse_buttons();
        if let App::Ready(game) = &mut self.app {
            game.audio.set_output(false);
        }
    }

    pub(crate) fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Touch(finger) => {
                with_overlay(|o| o.controls.gamepad = false);
                self.touch(event_loop, id, finger);
            }
            WindowEvent::KeyboardInput { event: key, .. }
                if key.physical_key == PhysicalKey::Code(KeyCode::BrowserBack) =>
            {
                if key.state == ElementState::Pressed && !key.repeat {
                    self.back();
                }
            }
            WindowEvent::KeyboardInput { event: key, .. }
                if matches!(key.physical_key, PhysicalKey::Unidentified(NativeKeyCode::Android(_))) =>
            {
                if let PhysicalKey::Unidentified(NativeKeyCode::Android(code)) = key.physical_key
                    && let Some(button) = pad_button(code)
                {
                    let down = key.state == ElementState::Pressed;
                    GAMEPAD.with(|g| {
                        let mut state = g.get();
                        state.set(button, down);
                        if button == PadButton::LeftTrigger {
                            state.triggers[0] = if down { 1.0 } else { 0.0 };
                        }
                        if button == PadButton::RightTrigger {
                            state.triggers[1] = if down { 1.0 } else { 0.0 };
                        }
                        g.set(state);
                    });
                    with_overlay(|o| o.controls.gamepad = true);
                }
            }
            event => self.app.window_event(event_loop, id, event),
        }
    }

    pub(crate) fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::Motion { axis, value } = event {
            // Android's axis numbers (MotionEvent.AXIS_*); sticks point up as minus.
            let v = value as f32;
            GAMEPAD.with(|g| {
                let mut s = g.get();
                match axis {
                    0 => s.left[0] = v,
                    1 => s.left[1] = -v,
                    11 => s.right[0] = v,
                    14 => s.right[1] = -v,
                    17 | 23 => s.triggers[0] = v.max(0.0),
                    18 | 22 => s.triggers[1] = v.max(0.0),
                    15 => {
                        s.set(PadButton::DPadLeft, v < -0.5);
                        s.set(PadButton::DPadRight, v > 0.5);
                    }
                    16 => {
                        s.set(PadButton::DPadUp, v < -0.5);
                        s.set(PadButton::DPadDown, v > 0.5);
                    }
                    _ => {}
                }
                g.set(s);
            });
            if v.abs() > 0.3 {
                with_overlay(|o| o.controls.gamepad = true);
            }
            return;
        }
        self.app.device_event(event_loop, id, event);
    }

    pub(crate) fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        jni::gyroscope(false);
        self.app.exiting(event_loop);
    }

    /// The back gesture: it leaves the editor or the menu, else does what Escape does.
    pub(crate) fn back(&mut self) {
        let closed = with_overlay(|o| {
            if let Some(editor) = &mut o.editor {
                if !editor.finish_typing() {
                    o.editor = None;
                    o.controls = load_controls_into(&o.controls);
                }
                true
            } else if o.menu.open {
                o.menu.open = false;
                true
            } else {
                false
            }
        })
        .unwrap_or(false);
        if closed {
            jni::show_keyboard(false);
            self.keyboard = false;
            return;
        }
        if let App::Ready(game) = &mut self.app
            && let Err(e) = game.pad_back()
        {
            log::warn!("Horizon: back: {e}");
        }
    }

    pub(crate) fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let screen = self.screen();
        let in_game = self.in_game() && !self.editing();
        if in_game != self.was_in_game {
            self.was_in_game = in_game;
            if !in_game {
                self.release_mouse_buttons();
            }
        }
        let enabled = self.enabled;
        let gyroscope = with_overlay(|o| {
            if o.screen != screen || o.settings.fitted == 0.0 {
                // A new screen: Auto fits the layout to it again.
                o.settings.fitted = o.controls.fitting_size(screen);
            }
            o.controls.size = o.settings.size.unwrap_or(o.settings.fitted);
            o.screen = screen;
            o.in_game = in_game;
            o.shown = enabled;
            if in_game || o.editor.is_some() {
                o.pointer = None;
            } else if o.pointer.is_none() {
                // A screen opens with the pointer in the middle, as Zalith's does.
                o.pointer = Some((screen.width / 2.0, screen.height / 2.0));
            }
            o.settings.gyroscope
        })
        .unwrap_or(false);
        let (gx, gy) = jni::take_gyro();
        if gyroscope
            && in_game
            && let App::Ready(game) = &mut self.app
        {
            game.mouse
                .on_move(0.0, 0.0, f64::from(gx * GYRO), f64::from(-gy * GYRO), true, true);
        }
        self.typed();
        self.timers(event_loop, now);
        for (button, _) in self.releases.extract_if(.., |(_, at)| now >= *at) {
            if let App::Ready(game) = &mut self.app {
                let _ = game.play_mouse(button, false);
            }
        }
        self.apply_control_events(event_loop);
        if let Some((lines, next)) = self.scrolling
            && now >= next
        {
            self.wheel(event_loop, lines);
            self.scrolling = Some((lines, now + Duration::from_millis(80)));
        }
        let editing = self.editing();
        if let App::Ready(game) = &mut self.app {
            let typing = !editing
                && (game.pad_typing()
                    || game.title.as_mut().is_some_and(|t| t.focused_field().is_some()));
            let want = typing || self.forced_keyboard || (editing && self.keyboard);
            if want != self.keyboard {
                self.keyboard = want;
                jni::show_keyboard(want);
            }
        }
        self.app.about_to_wait(event_loop);
    }

    fn typed(&mut self) {
        let typed = jni::take_typed();
        if self.editing() {
            for t in typed {
                let done = with_overlay(|o| {
                    let editor = o.editor.as_mut()?;
                    match &t {
                        Typed::Text(text) => editor.type_text(text),
                        Typed::Backspace => editor.backspace(),
                        Typed::Enter | Typed::Back => return Some(editor.finish_typing()),
                    }
                    None
                })
                .flatten();
                if done.is_some() {
                    self.keyboard = false;
                    jni::show_keyboard(false);
                }
            }
            return;
        }
        let back = typed.contains(&Typed::Back);
        let App::Ready(game) = &mut self.app else {
            return;
        };
        for t in typed {
            match t {
                Typed::Text(text) => game.pad_osk_output(osk::Output::Text(text)),
                Typed::Backspace => game.pad_osk_output(osk::Output::Backspace),
                Typed::Enter => game.pad_osk_output(osk::Output::Enter),
                Typed::Back => {}
            }
        }
        if back {
            self.back();
        }
    }

    /// Long presses: the menu pointer's button, the hotbar's drop and a gesture's hold.
    fn timers(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let mut hold = false;
        let mut drop = false;
        let mut gesture = false;
        for contact in self.contacts.values_mut() {
            if contact.moved || now - contact.since < LONG_PRESS {
                continue;
            }
            match &mut contact.role {
                Role::Pointer {
                    move_only: false,
                    held,
                } if !*held => {
                    *held = true;
                    hold = true;
                }
                Role::Hotbar { dropped, .. } if !*dropped => {
                    *dropped = true;
                    drop = true;
                }
                Role::Look {
                    gestures: true,
                    held,
                } if held.is_none() => {
                    *held = Some(MouseButton::Left);
                    gesture = true;
                }
                _ => {}
            }
        }
        let Some(id) = self.window().map(|w| w.id()) else {
            return;
        };
        if hold {
            self.menu_mouse(event_loop, id, MouseButton::Left, true);
        }
        if let App::Ready(game) = &mut self.app {
            if drop {
                tap_key(game, KeyCode::KeyQ);
                jni::vibrate(15);
            }
            if gesture {
                let _ = game.play_mouse(MouseButton::Left, true);
            }
        }
    }

    fn pointer(&self) -> (f32, f32) {
        with_overlay(|o| o.pointer).flatten().unwrap_or((0.0, 0.0))
    }

    /// The cursor where the menu's pointer is, and a button pressed or released there.
    fn menu_mouse(&mut self, event_loop: &ActiveEventLoop, id: WindowId, button: MouseButton, pressed: bool) {
        let at = self.pointer();
        let device_id = DeviceId::dummy();
        self.app.window_event(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id,
                position: winit::dpi::PhysicalPosition::new(f64::from(at.0), f64::from(at.1)),
            },
        );
        self.app.window_event(
            event_loop,
            id,
            WindowEvent::MouseInput {
                device_id,
                state: if pressed {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                },
                button,
            },
        );
    }

    fn move_pointer(&mut self, event_loop: &ActiveEventLoop, id: WindowId, delta: (f32, f32)) {
        let screen = self.screen();
        let at = with_overlay(|o| {
            let p = o.pointer.unwrap_or((screen.width / 2.0, screen.height / 2.0));
            let p = (
                (p.0 + delta.0).clamp(0.0, screen.width),
                (p.1 + delta.1).clamp(0.0, screen.height),
            );
            o.pointer = Some(p);
            p
        })
        .unwrap_or_default();
        self.app.window_event(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: winit::dpi::PhysicalPosition::new(f64::from(at.0), f64::from(at.1)),
            },
        );
    }

    fn wheel(&mut self, event_loop: &ActiveEventLoop, lines: f32) {
        let Some(id) = self.window().map(|w| w.id()) else {
            return;
        };
        self.app.window_event(
            event_loop,
            id,
            WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, lines),
                phase: TouchPhase::Moved,
            },
        );
    }

    /// A mouse button from the controls: the game's in the world, the
    /// menu pointer's over a screen.
    fn mouse_button(&mut self, event_loop: &ActiveEventLoop, button: MouseButton, pressed: bool) {
        if pressed {
            self.buttons.push(button);
        } else {
            self.buttons.retain(|b| *b != button);
        }
        if self.in_game() {
            if let App::Ready(game) = &mut self.app {
                let _ = game.play_mouse(button, pressed);
            }
        } else if let Some(id) = self.window().map(|w| w.id()) {
            self.menu_mouse(event_loop, id, button, pressed);
        }
    }

    fn release_mouse_buttons(&mut self) {
        if let App::Ready(game) = &mut self.app {
            for button in std::mem::take(&mut self.buttons) {
                let _ = game.play_mouse(button, false);
            }
        }
    }

    /// What the controls asked for: keys, mouse buttons, the launcher's events.
    fn apply_control_events(&mut self, event_loop: &ActiveEventLoop) {
        let events = with_overlay(|o| o.controls.take_events()).unwrap_or_default();
        for event in events {
            match event {
                ControlEvent::Key { name, pressed } | ControlEvent::Launcher { name, pressed }
                    if mouse_button(&name).is_some() =>
                {
                    self.mouse_button(event_loop, mouse_button(&name).unwrap(), pressed);
                }
                ControlEvent::Key { name, pressed } => {
                    let Some(key) = glfw_key(&name) else {
                        log::warn!("Horizon: the control layout's key {name} has no key here");
                        continue;
                    };
                    if let App::Ready(game) = &mut self.app {
                        // As a keyboard's key goes (window_event): held for the F3
                        // combinations, the debug keys first, then the game's.
                        if pressed {
                            game.physical_keys.insert(key);
                        } else {
                            game.physical_keys.remove(&key);
                        }
                        let action = if pressed {
                            horizon_client::debug_keys::KeyAction::Press
                        } else {
                            horizon_client::debug_keys::KeyAction::Release
                        };
                        if game.debug_key(key, action) {
                            continue;
                        }
                        if key == KeyCode::Escape {
                            if pressed {
                                let _ = game.pad_back();
                            }
                        } else {
                            let _ = game.mapped_key(key, pressed, false);
                        }
                    }
                }
                ControlEvent::Launcher { name, pressed } => match name.as_str() {
                    "launcher.event.switch_ime" if pressed => {
                        self.forced_keyboard = !self.forced_keyboard;
                    }
                    "launcher.event.scroll_up" | "launcher.event.scroll_down" => {
                        let lines = if name.ends_with("up") { 1.0 } else { -1.0 };
                        self.scrolling = pressed.then(|| (lines, Instant::now()));
                    }
                    "launcher.event.scroll_up.single" if pressed => self.wheel(event_loop, 1.0),
                    "launcher.event.scroll_down.single" if pressed => self.wheel(event_loop, -1.0),
                    "launcher.event.switch_menu" if pressed => {
                        with_overlay(|o| o.menu.open = !o.menu.open);
                    }
                    _ => {}
                },
                ControlEvent::SendText(text) => {
                    if let App::Ready(game) = &mut self.app {
                        if game.screen != Screen::Chat {
                            tap_key(game, KeyCode::KeyT);
                        }
                        game.pad_osk_output(osk::Output::Text(text));
                        game.pad_osk_output(osk::Output::Enter);
                    }
                }
            }
        }
    }

    /// A choice from the game menu.
    fn choose(&mut self, choice: Choice) {
        let home = self.home.clone();
        match choice {
            Choice::EditControls => {
                if self.in_game()
                    && let App::Ready(game) = &mut self.app
                {
                    // The world pauses behind the editor.
                    let _ = game.pad_back();
                }
                self.release_mouse_buttons();
                with_overlay(|o| {
                    o.menu.open = false;
                    let controls = std::mem::replace(&mut o.controls, Controls::default_layout());
                    o.editor = Some(Editor::new(controls));
                });
            }
            Choice::Gestures => {
                let on = with_overlay(|o| {
                    o.settings.gestures = !o.settings.gestures;
                    o.settings.gestures
                })
                .unwrap_or(false);
                set_flag(&home, "touchGestures", on);
            }
            Choice::Gyroscope => {
                let on = with_overlay(|o| {
                    o.settings.gyroscope = !o.settings.gyroscope;
                    o.settings.gyroscope
                })
                .unwrap_or(false);
                set_flag(&home, "touchGyroscope", on);
                jni::gyroscope(on);
            }
            Choice::Opacity => {
                let opacity = with_overlay(|o| {
                    o.settings.opacity = match o.settings.opacity {
                        v if v > 0.8 => 0.75,
                        v if v > 0.6 => 0.5,
                        v if v > 0.3 => 0.25,
                        _ => 1.0,
                    };
                    o.settings.opacity
                })
                .unwrap_or(1.0);
                set_preference(&home, "touchOpacity", Some(&opacity.to_string()));
            }
            Choice::HideControls => {
                with_overlay(|o| {
                    o.settings.controls_hidden = !o.settings.controls_hidden;
                    o.controls.release_all();
                });
            }
            Choice::Size => {
                let size = with_overlay(|o| {
                    // Auto, then the sizes from the largest down.
                    o.settings.size = match o.settings.size {
                        None => Some(1.2),
                        Some(v) if v > 0.65 => Some(((v - 0.1) * 10.0).round() / 10.0),
                        Some(_) => None,
                    };
                    o.controls.release_all();
                    o.settings.size
                })
                .flatten();
                set_preference(&home, "touchSize", size.map(|v| v.to_string()).as_deref());
            }
            Choice::Interface => {
                let on = with_overlay(|o| {
                    o.settings.larger_interface = !o.settings.larger_interface;
                    o.settings.larger_interface
                })
                .unwrap_or(false);
                set_flag(&home, "largerInterface", on);
                set_larger_interface(on);
                if let Some(w) = self.window() {
                    w.request_redraw();
                }
            }
            Choice::DebugSize => {
                let next = with_overlay(|o| {
                    o.settings.debug_scale = match o.settings.debug_scale {
                        0 => 3,
                        n @ 3..=5 => n + 1,
                        6 => -1,
                        _ => 0,
                    };
                    o.settings.debug_scale
                })
                .unwrap_or(0);
                if let App::Ready(game) = &mut self.app {
                    game.settings
                        .file
                        .set("debugGuiScale", horizon_client::options_file::V::Int(next));
                    game.settings.save(&Game::options_path());
                }
            }
            Choice::Keyboard => {
                self.forced_keyboard = !self.forced_keyboard;
                with_overlay(|o| o.menu.open = false);
            }
            Choice::Pause => {
                with_overlay(|o| o.menu.open = false);
                if let App::Ready(game) = &mut self.app {
                    let _ = game.pad_back();
                }
            }
            Choice::Close => {
                with_overlay(|o| o.menu.open = false);
            }
        }
    }

    fn touch(&mut self, event_loop: &ActiveEventLoop, id: WindowId, finger: Finger) {
        let at = (finger.location.x as f32, finger.location.y as f32);
        let now = Instant::now();
        let screen = self.screen();
        if self.enabled && self.editing() {
            self.edit_touch(finger.id, finger.phase, at, screen);
            return;
        }
        let in_game = self.in_game();
        let slop = SLOP * self.dp;
        match finger.phase {
            TouchPhase::Started => {
                let enabled = self.enabled;
                let claim = with_overlay(|o| {
                    if !enabled {
                        return Claim::None;
                    }
                    if o.menu.down(finger.id, at, screen) {
                        return Claim::Taken;
                    }
                    if o.settings.controls_hidden {
                        return Claim::None;
                    }
                    o.controls.down(finger.id, at, in_game, screen)
                })
                .unwrap_or(Claim::None);
                let gestures = with_overlay(|o| o.settings.gestures).unwrap_or(false);
                let role = match claim {
                    Claim::Taken => Role::Control,
                    Claim::MoveOnly if in_game => Role::Look {
                        gestures: false,
                        held: None,
                    },
                    Claim::MoveOnly => Role::Pointer {
                        move_only: true,
                        held: false,
                    },
                    Claim::None => match self.hotbar_slot(at) {
                        Some(slot) if in_game => {
                            self.select_slot(slot);
                            Role::Hotbar {
                                slot,
                                dropped: false,
                            }
                        }
                        _ if in_game => Role::Look { gestures, held: None },
                        _ => Role::Pointer {
                            move_only: false,
                            held: false,
                        },
                    },
                };
                self.contacts.insert(
                    finger.id,
                    Contact {
                        role,
                        start: at,
                        at,
                        since: now,
                        moved: false,
                    },
                );
            }
            TouchPhase::Moved => {
                if self.enabled {
                    with_overlay(|o| {
                        if o.menu.holds(finger.id) {
                            o.menu.moved(finger.id, at, screen);
                        } else {
                            o.controls.moved(finger.id, at, in_game, screen);
                        }
                    });
                }
                let hotbar = self.hotbar_slot(at);
                let Some(contact) = self.contacts.get_mut(&finger.id) else {
                    return;
                };
                let last = contact.at;
                contact.at = at;
                if !contact.moved && (at.0 - contact.start.0).hypot(at.1 - contact.start.1) > slop {
                    contact.moved = true;
                }
                let delta = (at.0 - last.0, at.1 - last.1);
                match &mut contact.role {
                    Role::Look { .. } => {
                        if let App::Ready(game) = &mut self.app {
                            game.mouse
                                .on_move(0.0, 0.0, f64::from(delta.0), f64::from(delta.1), true, true);
                            game.last_input = now;
                        }
                    }
                    Role::Pointer { .. } => self.move_pointer(event_loop, id, delta),
                    Role::Hotbar { slot, .. } => {
                        if let Some(over) = hotbar {
                            *slot = over;
                        }
                    }
                    Role::Control => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let mut choice = None;
                if self.enabled {
                    choice = with_overlay(|o| {
                        let choice = o.menu.up(finger.id, at);
                        o.controls.up(finger.id, screen);
                        choice
                    })
                    .flatten();
                    let ball = with_overlay(|o| o.menu.ball).unwrap_or((0.5, 0.05));
                    set_preference(&self.home, "touchMenuBall", Some(&format!("{},{}", ball.0, ball.1)));
                }
                let Some(contact) = self.contacts.remove(&finger.id) else {
                    return;
                };
                if let Some(choice) = choice {
                    self.choose(choice);
                }
                let cancelled = finger.phase == TouchPhase::Cancelled;
                let quick = !contact.moved && !cancelled && now - contact.since <= LONG_PRESS;
                match contact.role {
                    Role::Pointer {
                        move_only: false,
                        held,
                    } => {
                        if held {
                            self.menu_mouse(event_loop, id, MouseButton::Left, false);
                        } else if quick {
                            self.menu_mouse(event_loop, id, MouseButton::Left, true);
                            self.menu_mouse(event_loop, id, MouseButton::Left, false);
                        }
                    }
                    Role::Look { gestures, held } => {
                        if let App::Ready(game) = &mut self.app {
                            if let Some(button) = held {
                                let _ = game.play_mouse(button, false);
                            } else if gestures && quick {
                                // Zalith's gesture tap: the right button.
                                let _ = game.play_mouse(MouseButton::Right, true);
                                self.releases.push((MouseButton::Right, now + Duration::from_millis(60)));
                            }
                        }
                    }
                    Role::Hotbar { slot, dropped } if !dropped && !cancelled => {
                        let double = self
                            .last_slot_tap
                            .is_some_and(|(last, at)| last == slot && now - at < DOUBLE_TAP);
                        if double {
                            self.last_slot_tap = None;
                            if let App::Ready(game) = &mut self.app {
                                tap_key(game, KeyCode::KeyF);
                            }
                        } else {
                            self.last_slot_tap = Some((slot, now));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// A finger on the layout editor.
    fn edit_touch(&mut self, finger: u64, phase: TouchPhase, at: (f32, f32), screen: ControlScreen) {
        let outcome = with_overlay(|o| {
            let editor = o.editor.as_mut()?;
            Some(match phase {
                TouchPhase::Started => {
                    editor.down(finger, at, screen);
                    Outcome::None
                }
                TouchPhase::Moved => {
                    editor.moved(finger, at, screen);
                    Outcome::None
                }
                TouchPhase::Ended | TouchPhase::Cancelled => editor.up(finger, at, screen),
            })
        })
        .flatten()
        .unwrap_or(Outcome::None);
        match outcome {
            Outcome::None => {}
            Outcome::Keyboard(show) => {
                self.keyboard = show;
                jni::show_keyboard(show);
            }
            Outcome::Save => {
                let home = self.home.clone();
                let saved = with_overlay(|o| {
                    let editor = o.editor.take()?;
                    let result = save_layout(&home, editor.layout());
                    o.controls = editor.into_controls();
                    o.settings.fitted = 0.0;
                    Some(result)
                })
                .flatten();
                match saved {
                    Some(Ok(stem)) => log::info!("Horizon: saved the control layout {stem}"),
                    Some(Err(e)) => log::warn!("Horizon: the control layout was not saved: {e}"),
                    None => {}
                }
                self.keyboard = false;
                jni::show_keyboard(false);
            }
            Outcome::Exit => {
                let home = self.home.clone();
                with_overlay(|o| {
                    if let Some(editor) = o.editor.take() {
                        let language = editor.into_controls().language;
                        o.controls = load_controls(&home);
                        o.controls.language = language;
                        o.settings.fitted = 0.0;
                    }
                });
                self.keyboard = false;
                jni::show_keyboard(false);
            }
        }
    }

    /// The hotbar slot under `at` (physical pixels): vanilla's 9 slots of
    /// 20 GUI pixels, centred at the bottom.
    fn hotbar_slot(&self, at: (f32, f32)) -> Option<usize> {
        let screen = self.screen();
        let scale = HudBatch::new([screen.width as u32, screen.height as u32]).scale;
        let slot = 20.0 * scale;
        let left = (screen.width - 9.0 * slot) / 2.0;
        if at.1 < screen.height - slot || at.0 < left || at.0 >= left + 9.0 * slot {
            return None;
        }
        Some((((at.0 - left) / slot) as usize).min(8))
    }

    fn select_slot(&mut self, slot: usize) {
        if let App::Ready(game) = &mut self.app {
            game.selected = slot;
        }
    }
}

/// The controls as they were before an editing that was left unsaved.
fn load_controls_into(current: &Controls) -> Controls {
    let home = std::env::current_dir().unwrap_or_default();
    let mut controls = load_controls(&home);
    controls.language = current.language.clone();
    controls
}

/// GUI scales past vanilla's largest, for a phone's small screen: the GUI's
/// smallest height goes from vanilla's 240 GUI pixels to 160.
fn set_larger_interface(on: bool) {
    horizon_client::hud::GUI_MIN_HEIGHT.store(if on { 160 } else { 240 }, Ordering::Relaxed);
}

fn tap_key(game: &mut Game, key: KeyCode) {
    let _ = game.mapped_key(key, true, false);
    let _ = game.mapped_key(key, false, false);
}

/// Android's gamepad key codes (KeyEvent.KEYCODE_BUTTON_*) as the pad's buttons.
fn pad_button(code: u32) -> Option<PadButton> {
    Some(match code {
        96 => PadButton::South,
        97 => PadButton::East,
        99 => PadButton::West,
        100 => PadButton::North,
        102 => PadButton::LeftBumper,
        103 => PadButton::RightBumper,
        104 => PadButton::LeftTrigger,
        105 => PadButton::RightTrigger,
        106 => PadButton::LeftStick,
        107 => PadButton::RightStick,
        108 => PadButton::Start,
        109 | 110 => PadButton::Select,
        _ => return None,
    })
}

fn mouse_button(name: &str) -> Option<MouseButton> {
    Some(match name {
        "GLFW_MOUSE_BUTTON_LEFT" | "GLFW_MOUSE_BUTTON_1" => MouseButton::Left,
        "GLFW_MOUSE_BUTTON_RIGHT" | "GLFW_MOUSE_BUTTON_2" => MouseButton::Right,
        "GLFW_MOUSE_BUTTON_MIDDLE" | "GLFW_MOUSE_BUTTON_3" => MouseButton::Middle,
        "GLFW_MOUSE_BUTTON_4" => MouseButton::Back,
        "GLFW_MOUSE_BUTTON_5" => MouseButton::Forward,
        _ => return None,
    })
}

/// A GLFW key name (`GLFW_KEY_E`) as the key it names.
fn glfw_key(name: &str) -> Option<KeyCode> {
    use KeyCode as K;
    let key = name.strip_prefix("GLFW_KEY_")?;
    if key.len() == 1 {
        let c = key.as_bytes()[0];
        return Some(match c {
            b'A'..=b'Z' => [
                K::KeyA, K::KeyB, K::KeyC, K::KeyD, K::KeyE, K::KeyF, K::KeyG, K::KeyH, K::KeyI,
                K::KeyJ, K::KeyK, K::KeyL, K::KeyM, K::KeyN, K::KeyO, K::KeyP, K::KeyQ, K::KeyR,
                K::KeyS, K::KeyT, K::KeyU, K::KeyV, K::KeyW, K::KeyX, K::KeyY, K::KeyZ,
            ][usize::from(c - b'A')],
            b'0'..=b'9' => [
                K::Digit0, K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6,
                K::Digit7, K::Digit8, K::Digit9,
            ][usize::from(c - b'0')],
            _ => return None,
        });
    }
    if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<usize>().ok()) {
        return [
            K::F1, K::F2, K::F3, K::F4, K::F5, K::F6, K::F7, K::F8, K::F9, K::F10, K::F11, K::F12,
            K::F13, K::F14, K::F15, K::F16, K::F17, K::F18, K::F19, K::F20, K::F21, K::F22, K::F23,
            K::F24, K::F25,
        ]
        .get(n.wrapping_sub(1))
        .copied();
    }
    if let Some(n) = key.strip_prefix("KP_").and_then(|n| n.parse::<usize>().ok()) {
        return [
            K::Numpad0, K::Numpad1, K::Numpad2, K::Numpad3, K::Numpad4, K::Numpad5, K::Numpad6,
            K::Numpad7, K::Numpad8, K::Numpad9,
        ]
        .get(n)
        .copied();
    }
    Some(match key {
        "SPACE" => K::Space,
        "APOSTROPHE" => K::Quote,
        "COMMA" => K::Comma,
        "MINUS" => K::Minus,
        "PERIOD" => K::Period,
        "SLASH" => K::Slash,
        "SEMICOLON" => K::Semicolon,
        "EQUAL" => K::Equal,
        "LEFT_BRACKET" => K::BracketLeft,
        "BACKSLASH" => K::Backslash,
        "RIGHT_BRACKET" => K::BracketRight,
        "GRAVE_ACCENT" => K::Backquote,
        "WORLD_1" | "WORLD_2" => K::IntlBackslash,
        "ESCAPE" => K::Escape,
        "ENTER" => K::Enter,
        "TAB" => K::Tab,
        "BACKSPACE" => K::Backspace,
        "INSERT" => K::Insert,
        "DELETE" => K::Delete,
        "RIGHT" => K::ArrowRight,
        "LEFT" => K::ArrowLeft,
        "DOWN" => K::ArrowDown,
        "UP" => K::ArrowUp,
        "PAGE_UP" => K::PageUp,
        "PAGE_DOWN" => K::PageDown,
        "HOME" => K::Home,
        "END" => K::End,
        "CAPS_LOCK" => K::CapsLock,
        "SCROLL_LOCK" => K::ScrollLock,
        "NUM_LOCK" => K::NumLock,
        "PRINT_SCREEN" => K::PrintScreen,
        "PAUSE" => K::Pause,
        "KP_DECIMAL" => K::NumpadDecimal,
        "KP_DIVIDE" => K::NumpadDivide,
        "KP_MULTIPLY" => K::NumpadMultiply,
        "KP_SUBTRACT" => K::NumpadSubtract,
        "KP_ADD" => K::NumpadAdd,
        "KP_ENTER" => K::NumpadEnter,
        "KP_EQUAL" => K::NumpadEqual,
        "LEFT_SHIFT" => K::ShiftLeft,
        "LEFT_CONTROL" => K::ControlLeft,
        "LEFT_ALT" => K::AltLeft,
        "LEFT_SUPER" => K::SuperLeft,
        "RIGHT_SHIFT" => K::ShiftRight,
        "RIGHT_CONTROL" => K::ControlRight,
        "RIGHT_ALT" => K::AltRight,
        "RIGHT_SUPER" => K::SuperRight,
        "MENU" => K::ContextMenu,
        _ => return None,
    })
}

impl Game<'_> {
    /// The world is on screen with nothing over it: the game has the pointer.
    fn touch_playing(&mut self) -> bool {
        self.screen == Screen::Playing
            && self.title.is_none()
            && self.player.is_some()
            && self.sign_edit.is_none()
            && self.book.is_none()
            && self.pending_entry.is_none()
            && self.world_loader.is_none()
            && !self.dialog_open()
            && self.server_packs.prompt.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glfw_names_map_to_keys() {
        assert_eq!(glfw_key("GLFW_KEY_E"), Some(KeyCode::KeyE));
        assert_eq!(glfw_key("GLFW_KEY_7"), Some(KeyCode::Digit7));
        assert_eq!(glfw_key("GLFW_KEY_F3"), Some(KeyCode::F3));
        assert_eq!(glfw_key("GLFW_KEY_F25"), Some(KeyCode::F25));
        assert_eq!(glfw_key("GLFW_KEY_F26"), None);
        assert_eq!(glfw_key("GLFW_KEY_KP_5"), Some(KeyCode::Numpad5));
        assert_eq!(glfw_key("GLFW_KEY_LEFT_CONTROL"), Some(KeyCode::ControlLeft));
        assert_eq!(glfw_key("GLFW_KEY_UNKNOWN"), None);
        assert_eq!(mouse_button("GLFW_MOUSE_BUTTON_RIGHT"), Some(MouseButton::Right));
        assert_eq!(pad_button(96), Some(PadButton::South));
    }
}
