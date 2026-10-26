//! The game on Android (`android_main.rs` starts it after the launcher): the
//! touch screen, the soft keyboard, the back gesture and the window that goes
//! away while the app is in the background.
//!
//! Over a screen a finger is the mouse: a tap clicks, a drag drags, a long
//! press is a right click and two fingers scroll. In the world the touch
//! controls (`horizon_client::touch`) move, look, tap to use or attack and
//! hold to break; they drive the same keys and buttons a keyboard does.
use super::*;
use crate::android::jni::{self, Typed};
use horizon_client::osk;
use horizon_client::touch::{self, Button as TouchButton, Layout};
use std::cell::RefCell;
use std::collections::HashMap;
use winit::event::{Touch as Finger, TouchPhase};

/// A press this long without moving is a hold: breaking in the world, a
/// right click over a screen.
const HOLD: Duration = Duration::from_millis(320);
/// How far a finger moves before a tap becomes a drag, in dp.
const SLOP: f32 = 10.0;
/// Mouse counts per physical pixel a finger looks with.
const LOOK: f64 = 0.9;

thread_local! {
    /// What the controls show, for `draw_touch` in the frame's HUD.
    static OVERLAY: RefCell<Option<(Layout, touch::State)>> = const { RefCell::new(None) };
}

/// The touch controls over the HUD, while they are shown.
pub(crate) fn draw_touch(batch: &mut HudBatch, atlas: &horizon_client::hud::HudAtlas) {
    OVERLAY.with(|o| {
        if let Some((layout, state)) = &*o.borrow() {
            touch::draw(batch, atlas, layout, state);
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// The stick, centred where the thumb landed.
    Stick((f32, f32)),
    Button(TouchButton),
    /// Looking; `held` is the mouse button a hold pressed.
    Look { held: Option<MouseButton> },
    Slot,
    /// The mouse over a screen; `pressed` once a drag pressed the button.
    Mouse { pressed: bool, done: bool },
    /// A second finger over a screen: the wheel.
    Wheel,
    Ignore,
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
    /// Forward, back, left, right and sprint, as the stick holds them.
    stick_keys: [bool; 5],
    releases: Vec<(MouseButton, Instant)>,
    keyboard: bool,
    enabled: bool,
    /// Physical pixels per dp.
    dp: f32,
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
        Ok(Self {
            app: loading_app(options, settings, vanilla_root, None, started)?,
            contacts: HashMap::new(),
            stick_keys: [false; 5],
            releases: Vec::new(),
            keyboard: false,
            enabled,
            dp,
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
        if let Some(w) = self.window() {
            w.request_redraw();
        }
    }

    /// The app left the screen and its window with it.
    pub(crate) fn suspended(&mut self) {
        jni::show_keyboard(false);
        self.keyboard = false;
        if let Some(renderer) = self.renderer() {
            renderer.release_surface();
            renderer.suspended = true;
        }
        if let App::Ready(game) = &mut self.app {
            self.contacts.clear();
            release_play(game, &mut self.stick_keys);
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
        let typed = jni::take_typed();
        if let App::Ready(game) = &mut self.app {
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
            let typing = game.pad_typing()
                || game.title.as_mut().is_some_and(|t| t.focused_field().is_some());
            if typing != self.keyboard {
                self.keyboard = typing;
                jni::show_keyboard(typing);
            }
            let playing = self.enabled && game.touch_playing();
            if !playing {
                release_play(game, &mut self.stick_keys);
                for contact in self.contacts.values_mut() {
                    if matches!(contact.role, Role::Stick(_) | Role::Look { .. } | Role::Button(_)) {
                        contact.role = Role::Ignore;
                    }
                }
            }
            // A finger resting on the world: break, or use what is held over time.
            for contact in self.contacts.values_mut() {
                if let Role::Look { held: None } = contact.role
                    && !contact.moved
                    && now - contact.since >= HOLD
                {
                    let button = if game.holds_item_used_over_time() {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    };
                    contact.role = Role::Look { held: Some(button) };
                    let _ = game.play_mouse(button, true);
                }
            }
            for (button, _) in self.releases.extract_if(.., |(_, at)| now >= *at) {
                let _ = game.play_mouse(button, false);
            }
            let overlay = playing.then(|| {
                let (layout, sneaking) = game.touch_layout(self.dp);
                let mut state = touch::State {
                    opacity: 0.85,
                    sneaking,
                    ..Default::default()
                };
                for c in self.contacts.values() {
                    match c.role {
                        Role::Stick(centre) => {
                            state.stick = Some((centre, clamp_stick(centre, c.at, layout.stick)));
                        }
                        Role::Button(b) => state.pressed.push(b),
                        _ => {}
                    }
                }
                (layout, state)
            });
            OVERLAY.with(|o| *o.borrow_mut() = overlay);
        }
        // Over a screen, a finger resting without moving is a right click.
        let mut right_clicks = Vec::new();
        for contact in self.contacts.values_mut() {
            if let Role::Mouse {
                pressed: false,
                done: false,
            } = contact.role
                && !contact.moved
                && now - contact.since >= HOLD
            {
                contact.role = Role::Mouse {
                    pressed: false,
                    done: true,
                };
                right_clicks.push(contact.at);
            }
        }
        if let Some(id) = self.window().map(|w| w.id()) {
            for at in right_clicks {
                self.mouse(event_loop, id, at, Some((MouseButton::Right, true)));
                self.mouse(event_loop, id, at, Some((MouseButton::Right, false)));
                jni::vibrate(15);
            }
        }
        self.app.about_to_wait(event_loop);
    }

    /// The cursor at `at` (physical pixels), and a button's press or release.
    fn mouse(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        at: (f32, f32),
        button: Option<(MouseButton, bool)>,
    ) {
        let device_id = DeviceId::dummy();
        self.app.window_event(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id,
                position: winit::dpi::PhysicalPosition::new(f64::from(at.0), f64::from(at.1)),
            },
        );
        if let Some((button, pressed)) = button {
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
    }

    fn touch(&mut self, event_loop: &ActiveEventLoop, id: WindowId, finger: Finger) {
        let at = (finger.location.x as f32, finger.location.y as f32);
        let now = Instant::now();
        let slop = SLOP * self.dp;
        match finger.phase {
            TouchPhase::Started => {
                let role = self.role_for(at);
                if let Role::Button(b) = role
                    && let App::Ready(game) = &mut self.app
                {
                    jni::vibrate(10);
                    game.touch_button(b, true);
                }
                if let (Role::Slot, App::Ready(game)) = (role, &mut self.app) {
                    let (layout, _) = game.touch_layout(self.dp);
                    if let Some(slot) = layout.slot_at(at) {
                        game.selected = slot;
                    }
                }
                if matches!(role, Role::Mouse { .. }) {
                    self.mouse(event_loop, id, at, None);
                }
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
                let Some(contact) = self.contacts.get_mut(&finger.id) else {
                    return;
                };
                let last = contact.at;
                contact.at = at;
                if !contact.moved && (at.0 - contact.start.0).hypot(at.1 - contact.start.1) > slop {
                    contact.moved = true;
                }
                let (role, moved, start) = (contact.role, contact.moved, contact.start);
                match role {
                    Role::Stick(centre) => {
                        if let App::Ready(game) = &mut self.app {
                            let (layout, _) = game.touch_layout(self.dp);
                            let thumb = clamp_stick(centre, at, layout.stick);
                            let keys = touch::stick_keys(
                                (thumb.0 - centre.0) / layout.stick,
                                (thumb.1 - centre.1) / layout.stick,
                            );
                            set_stick_keys(game, &mut self.stick_keys, keys);
                        }
                    }
                    Role::Look { .. } => {
                        if let App::Ready(game) = &mut self.app {
                            let (dx, dy) = (f64::from(at.0 - last.0), f64::from(at.1 - last.1));
                            game.mouse.on_move(0.0, 0.0, dx * LOOK, dy * LOOK, true, true);
                            game.last_input = now;
                        }
                    }
                    Role::Mouse { pressed, done } if moved && !done => {
                        if !pressed {
                            self.mouse(event_loop, id, start, Some((MouseButton::Left, true)));
                            if let Some(c) = self.contacts.get_mut(&finger.id) {
                                c.role = Role::Mouse {
                                    pressed: true,
                                    done: false,
                                };
                            }
                        }
                        self.mouse(event_loop, id, at, None);
                    }
                    Role::Wheel => {
                        let lines = (last.1 - at.1) / (36.0 * self.dp);
                        if lines != 0.0 {
                            self.app.window_event(
                                event_loop,
                                id,
                                WindowEvent::MouseWheel {
                                    device_id: DeviceId::dummy(),
                                    delta: MouseScrollDelta::LineDelta(0.0, -lines),
                                    phase: TouchPhase::Moved,
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(contact) = self.contacts.remove(&finger.id) else {
                    return;
                };
                let cancelled = finger.phase == TouchPhase::Cancelled;
                match contact.role {
                    Role::Stick(_) => {
                        if let App::Ready(game) = &mut self.app {
                            set_stick_keys(game, &mut self.stick_keys, [false; 5]);
                        }
                    }
                    Role::Button(b) => {
                        if let App::Ready(game) = &mut self.app {
                            game.touch_button(b, false);
                        }
                    }
                    Role::Look { held } => {
                        if let App::Ready(game) = &mut self.app {
                            if let Some(button) = held {
                                let _ = game.play_mouse(button, false);
                            } else if !contact.moved && !cancelled {
                                // A tap: hit what is under the crosshair, else use the hand.
                                let button = if game.crosshair_living().is_some() {
                                    MouseButton::Left
                                } else {
                                    MouseButton::Right
                                };
                                let _ = game.play_mouse(button, true);
                                self.releases.push((button, now + Duration::from_millis(110)));
                            }
                        }
                    }
                    Role::Mouse { pressed, done } => {
                        if pressed {
                            self.mouse(event_loop, id, contact.at, Some((MouseButton::Left, false)));
                        } else if !done && !cancelled {
                            self.mouse(event_loop, id, contact.at, Some((MouseButton::Left, true)));
                            self.mouse(event_loop, id, contact.at, Some((MouseButton::Left, false)));
                        }
                    }
                    Role::Slot | Role::Wheel | Role::Ignore => {}
                }
            }
        }
    }

    /// What a finger landing at `at` does.
    fn role_for(&mut self, at: (f32, f32)) -> Role {
        let enabled = self.enabled;
        let dp = self.dp;
        let others = self.contacts.values().any(|c| matches!(c.role, Role::Mouse { .. }));
        let App::Ready(game) = &mut self.app else {
            return if others { Role::Wheel } else { Role::Mouse { pressed: false, done: false } };
        };
        if !(enabled && game.touch_playing()) {
            if others {
                // Two fingers scroll; the first one's click is off.
                for c in self.contacts.values_mut() {
                    if let Role::Mouse { pressed: false, .. } = c.role {
                        c.role = Role::Mouse {
                            pressed: false,
                            done: true,
                        };
                    }
                }
                return Role::Wheel;
            }
            return Role::Mouse {
                pressed: false,
                done: false,
            };
        }
        let (layout, _) = game.touch_layout(dp);
        let height = game.window.as_ref().map_or(1.0, |w| w.inner_size().height as f32);
        if let Some(b) = layout.button_at(at) {
            Role::Button(b)
        } else if layout.slot_at(at).is_some() {
            Role::Slot
        } else if layout.in_stick_zone(at, height)
            && !self.contacts.values().any(|c| matches!(c.role, Role::Stick(_)))
        {
            Role::Stick(at)
        } else {
            Role::Look { held: None }
        }
    }
}

fn clamp_stick(centre: (f32, f32), at: (f32, f32), radius: f32) -> (f32, f32) {
    let (dx, dy) = (at.0 - centre.0, at.1 - centre.1);
    let length = dx.hypot(dy);
    if length <= radius {
        at
    } else {
        (centre.0 + dx / length * radius, centre.1 + dy / length * radius)
    }
}

const STICK_KEYS: [KeyCode; 5] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::ControlLeft,
];

fn set_stick_keys(game: &mut Game, held: &mut [bool; 5], keys: [bool; 5]) {
    for i in 0..5 {
        if held[i] != keys[i] {
            held[i] = keys[i];
            let _ = game.mapped_key(STICK_KEYS[i], keys[i], false);
        }
    }
}

/// Nothing the controls hold stays held behind a screen.
fn release_play(game: &mut Game, held: &mut [bool; 5]) {
    set_stick_keys(game, held, [false; 5]);
    for button in [MouseButton::Left, MouseButton::Right] {
        let down = match button {
            MouseButton::Left => game.left,
            _ => game.right,
        };
        if down {
            let _ = game.play_mouse(button, false);
        }
    }
}

impl Game<'_> {
    /// The world is on screen with nothing over it: the touch controls act.
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

    /// The controls' layout on this window, and whether the player sneaks.
    fn touch_layout(&self, dp: f32) -> (Layout, bool) {
        let size = self
            .renderer
            .as_ref()
            .map_or([1, 1], |r| [r.config.width, r.config.height]);
        let scale = HudBatch::new(size).scale;
        let layout = Layout::new(size[0] as f32, size[1] as f32, dp, 1.0, scale);
        (layout, self.keys.contains(&KeyCode::ShiftLeft))
    }

    /// The held stack is used over time (food, a bow, a spyglass): a hold uses it.
    fn holds_item_used_over_time(&self) -> bool {
        self.held().is_some_and(|stack| {
            let view = horizon_client::item_render::stack_view(stack);
            item_draw::use_duration(&self.pack.item_renderer.models.components(&view)) > 0
        })
    }

    fn touch_button(&mut self, button: TouchButton, down: bool) {
        let tap = |game: &mut Self, key: KeyCode| {
            if down {
                let _ = game.mapped_key(key, true, false);
                let _ = game.mapped_key(key, false, false);
            }
        };
        match button {
            TouchButton::Jump => {
                let _ = self.mapped_key(KeyCode::Space, down, false);
            }
            TouchButton::Sneak => {
                if down {
                    let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
                    let _ = self.mapped_key(KeyCode::ShiftLeft, !sneaking, false);
                }
            }
            TouchButton::Inventory => tap(self, KeyCode::KeyE),
            TouchButton::Drop => tap(self, KeyCode::KeyQ),
            TouchButton::Chat => tap(self, KeyCode::KeyT),
            TouchButton::Perspective => tap(self, KeyCode::F5),
            TouchButton::Pause => {
                if down {
                    let _ = self.pad_back();
                }
            }
        }
    }
}
