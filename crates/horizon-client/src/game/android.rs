//! The game on Android (`android_main.rs` starts it after the launcher): the
//! touch screen, the soft keyboard, the back gesture and the window that goes
//! away while the app is in the background.
//!
//! Touch works as in Zalith Launcher 2. The control layout
//! (`horizon_client::controls`) takes the fingers that land on its buttons
//! and joysticks. In the world any other finger looks around, and the hotbar
//! takes taps (select), double taps (swap hands) and long presses (drop).
//! Over a screen a finger moves a mouse pointer the way a touchpad does: a
//! tap clicks where it points and a long press holds the button.
use super::*;
use crate::android::jni::{self, Typed};
use horizon_client::controls::{self, Claim, Controls, Event as ControlEvent, Screen as ControlScreen};
use horizon_client::osk;
use std::cell::RefCell;
use std::collections::HashMap;
use winit::event::{Touch as Finger, TouchPhase};

/// Zalith's long press and double tap timeouts.
const LONG_PRESS: Duration = Duration::from_millis(300);
const DOUBLE_TAP: Duration = Duration::from_millis(300);
/// How far a finger moves before a tap is a drag, in dp.
const SLOP: f32 = 8.0;

/// What the controls show, for `draw_touch` in the frame's HUD.
pub(crate) struct Overlay {
    pub controls: Controls,
    pub screen: ControlScreen,
    pub in_game: bool,
    /// The menu's mouse pointer, physical pixels, while a screen is open.
    pub pointer: Option<(f32, f32)>,
    pub shown: bool,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

/// The touch controls and the menu's pointer over the HUD.
pub(crate) fn draw_touch(batch: &mut HudBatch, atlas: &horizon_client::hud::HudAtlas) {
    OVERLAY.with(|o| {
        if let Some(o) = &mut *o.borrow_mut()
            && o.shown
        {
            o.controls.draw(batch, atlas, o.in_game, o.screen, 1.0);
            if let Some(at) = o.pointer {
                controls::draw_pointer(batch, atlas, at, 24.0 * o.screen.density);
            }
        }
    });
}

fn with_overlay<R>(f: impl FnOnce(&mut Overlay) -> R) -> Option<R> {
    OVERLAY.with(|o| o.borrow_mut().as_mut().map(f))
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// A control has it.
    Control,
    /// Looking around in the world; a penetrable button may hold it too.
    Look,
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
    contacts: HashMap<u64, Contact>,
    /// Mouse buttons the controls hold down.
    buttons: Vec<MouseButton>,
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
fn load_controls(home: &std::path::Path) -> Controls {
    let name = horizon_client::ui_theme::preference(home, "controlLayout").unwrap_or_default();
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
        let enabled = horizon_client::ui_theme::preference(&home, "touchControls")
            .is_none_or(|v| v != "false");
        let dp = jni::platform("density", "")
            .and_then(|d| d.parse().ok())
            .unwrap_or(2.0);
        let dark = jni::platform("dark", "").as_deref() == Some("1");
        let mut controls = load_controls(&home);
        controls.language = jni::platform("language", "").unwrap_or_default();
        OVERLAY.with(|o| {
            *o.borrow_mut() = Some(Overlay {
                controls,
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
            contacts: HashMap::new(),
            buttons: Vec::new(),
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
        if let Some(w) = self.window() {
            w.request_redraw();
        }
    }

    /// The app left the screen and its window with it.
    pub(crate) fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        jni::show_keyboard(false);
        self.keyboard = false;
        self.forced_keyboard = false;
        if let Some(renderer) = self.renderer() {
            renderer.release_surface();
            renderer.suspended = true;
        }
        self.contacts.clear();
        with_overlay(|o| o.controls.release_all());
        self.apply_control_events(event_loop);
        if let App::Ready(game) = &mut self.app {
            game.audio.set_output(false);
        }
    }

    pub(crate) fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Touch(finger) => self.touch(event_loop, id, finger),
            WindowEvent::KeyboardInput { event: key, .. }
                if key.physical_key == PhysicalKey::Code(KeyCode::BrowserBack) =>
            {
                if key.state == ElementState::Pressed && !key.repeat {
                    self.back();
                }
            }
            event => self.app.window_event(event_loop, id, event),
        }
    }

    pub(crate) fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.app.device_event(event_loop, id, event);
    }

    pub(crate) fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.app.exiting(event_loop);
    }

    /// The back gesture: what Escape does.
    pub(crate) fn back(&mut self) {
        if let App::Ready(game) = &mut self.app
            && let Err(e) = game.pad_back()
        {
            log::warn!("Horizon: back: {e}");
        }
    }

    pub(crate) fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let screen = self.screen();
        let in_game = self.in_game();
        if in_game != self.was_in_game {
            self.was_in_game = in_game;
            if !in_game {
                self.release_mouse_buttons();
            }
        }
        let enabled = self.enabled;
        with_overlay(|o| {
            o.screen = screen;
            o.in_game = in_game;
            o.shown = enabled;
            if in_game {
                o.pointer = None;
            } else if o.pointer.is_none() {
                // A screen opens with the pointer in the middle, as Zalith's does.
                o.pointer = Some((screen.width / 2.0, screen.height / 2.0));
            }
        });
        self.typed();
        self.timers(event_loop, now);
        self.apply_control_events(event_loop);
        if let Some((lines, next)) = self.scrolling
            && now >= next
        {
            self.wheel(event_loop, lines);
            self.scrolling = Some((lines, now + Duration::from_millis(80)));
        }
        if let App::Ready(game) = &mut self.app {
            let typing = game.pad_typing()
                || game.title.as_mut().is_some_and(|t| t.focused_field().is_some());
            let want = typing || self.forced_keyboard;
            if want != self.keyboard {
                self.keyboard = want;
                jni::show_keyboard(want);
            }
        }
        self.app.about_to_wait(event_loop);
    }

    fn typed(&mut self) {
        let typed = jni::take_typed();
        let App::Ready(game) = &mut self.app else {
            return;
        };
        for t in typed {
            match t {
                Typed::Text(text) => game.pad_osk_output(osk::Output::Text(text)),
                Typed::Backspace => game.pad_osk_output(osk::Output::Backspace),
                Typed::Enter => game.pad_osk_output(osk::Output::Enter),
                Typed::Back => {
                    if let Err(e) = game.pad_back() {
                        log::warn!("Horizon: back: {e}");
                    }
                }
            }
        }
    }

    /// Long presses: the menu pointer's button and the hotbar's drop.
    fn timers(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let mut hold = false;
        let mut drop = false;
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
                _ => {}
            }
        }
        let Some(id) = self.window().map(|w| w.id()) else {
            return;
        };
        if hold {
            self.menu_mouse(event_loop, id, MouseButton::Left, true);
        }
        if drop && let App::Ready(game) = &mut self.app {
            tap_key(game, KeyCode::KeyQ);
            jni::vibrate(15);
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
                    "launcher.event.switch_menu" if pressed => self.back(),
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

    fn touch(&mut self, event_loop: &ActiveEventLoop, id: WindowId, finger: Finger) {
        let at = (finger.location.x as f32, finger.location.y as f32);
        let now = Instant::now();
        let screen = self.screen();
        let in_game = self.in_game();
        let slop = SLOP * self.dp;
        match finger.phase {
            TouchPhase::Started => {
                let claim = if self.enabled {
                    with_overlay(|o| o.controls.down(finger.id, at, in_game, screen)).unwrap_or(Claim::None)
                } else {
                    Claim::None
                };
                let role = match claim {
                    Claim::Taken => Role::Control,
                    Claim::MoveOnly if in_game => Role::Look,
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
                        _ if in_game => Role::Look,
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
                    with_overlay(|o| o.controls.moved(finger.id, at, in_game, screen));
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
                    Role::Look => {
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
                if self.enabled {
                    with_overlay(|o| o.controls.up(finger.id, screen));
                }
                let Some(contact) = self.contacts.remove(&finger.id) else {
                    return;
                };
                let cancelled = finger.phase == TouchPhase::Cancelled;
                match contact.role {
                    Role::Pointer {
                        move_only: false,
                        held,
                    } => {
                        if held {
                            self.menu_mouse(event_loop, id, MouseButton::Left, false);
                        } else if !contact.moved && !cancelled && now - contact.since <= LONG_PRESS {
                            self.menu_mouse(event_loop, id, MouseButton::Left, true);
                            self.menu_mouse(event_loop, id, MouseButton::Left, false);
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

fn tap_key(game: &mut Game, key: KeyCode) {
    let _ = game.mapped_key(key, true, false);
    let _ = game.mapped_key(key, false, false);
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
    }
}
