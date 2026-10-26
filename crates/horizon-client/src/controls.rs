//! Touch controls the way Zalith Launcher 2 has them, read from its layout
//! files (`controls/layout.rs`): layers of buttons that send keys, mouse
//! buttons and the launcher's own events, joysticks, and text. A finger
//! that lands on no control is the game's (looking, or the menu's pointer).
//!
//! The rules follow Zalith's: the topmost button under a finger takes it; a
//! swipeable button is pressed by a finger sliding onto it and released when
//! it leaves; a toggle flips with each press; a penetrable one lets the
//! finger look around too. Layer events act when a button goes down.
//! Everything is in physical pixels.
pub mod editor;
pub mod menu;
mod widgets;
pub mod layout;

use crate::hud::{HudAtlas, HudBatch};
use crate::ui_theme::{Face, Ui};
use layout::*;
use std::collections::HashMap;

/// The layout Horizon starts with: Zalith Launcher 2's default.
pub const DEFAULT_LAYOUT: &str = include_str!("controls/default_layout.json");

/// Compose's text size where a style sets none (Material's body text), in sp.
const TEXT_SP: f32 = 16.0;

/// The window the controls are on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub width: f32,
    pub height: f32,
    /// Physical pixels per dp.
    pub density: f32,
    /// The system's dark theme (a style may have a dark variant).
    pub dark: bool,
}

/// What the controls ask of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A key or mouse button by its GLFW name.
    Key { name: String, pressed: bool },
    /// One of the launcher's (`launcher.event.*`).
    Launcher { name: String, pressed: bool },
    SendText(String),
}

/// Who a finger landing somewhere belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// No control: the game's.
    None,
    /// A control's alone.
    Taken,
    /// A penetrable button's, and the game may look with it.
    MoveOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn contains(&self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Behavior {
    Press,
    Swipe,
    Toggle,
}

impl Behavior {
    fn of(b: &Button) -> Self {
        if b.is_toggleable {
            Behavior::Toggle
        } else if b.is_swipple {
            Behavior::Swipe
        } else {
            Behavior::Press
        }
    }
}

type Id = (usize, usize);

#[derive(Default)]
struct Finger {
    active: Vec<Id>,
    swipe_chain: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Stick {
    direction: Option<Direction>,
    knob: (f32, f32),
    locked: bool,
    can_lock: bool,
    finger: Option<u64>,
    last: (f32, f32),
}

pub struct Controls {
    pub layout: Layout,
    hidden: Vec<bool>,
    pressed: Vec<Vec<bool>>,
    sticks: Vec<Vec<Stick>>,
    fingers: HashMap<u64, Finger>,
    /// Sizes of wrap content widgets, measured as they were drawn.
    measured: HashMap<String, (f32, f32)>,
    events: Vec<Event>,
    /// The system's language, for the buttons' translated text.
    pub language: String,
    /// Layers that hide while a gamepad plays do so.
    pub gamepad: bool,
    /// Every layer and widget, hidden or not (the editor).
    show_all: bool,
}

impl Controls {
    pub fn new(layout: Layout) -> Self {
        Self {
            hidden: layout.layers.iter().map(|l| l.hide).collect(),
            pressed: layout
                .layers
                .iter()
                .map(|l| vec![false; l.normal_buttons.len()])
                .collect(),
            sticks: layout
                .layers
                .iter()
                .map(|l| vec![Stick::default(); l.joystick_buttons.len()])
                .collect(),
            layout,
            fingers: HashMap::new(),
            measured: HashMap::new(),
            events: Vec::new(),
            language: String::new(),
            gamepad: false,
            show_all: false,
        }
    }

    pub fn default_layout() -> Self {
        Self::new(Layout::parse(DEFAULT_LAYOUT).expect("the default layout"))
    }

    /// What the controls asked for since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn layer_shown(&self, layer: usize, in_game: bool) -> bool {
        let l = &self.layout.layers[layer];
        self.show_all
            || (!self.hidden[layer]
                && l.visibility_type.shown(in_game)
                && !(self.gamepad && l.hide_when_gamepad))
    }

    fn shown(&self, visibility: Visibility, in_game: bool) -> bool {
        self.show_all || visibility.shown(in_game)
    }

    fn size(&self, size: &Size, uuid: &str, screen: Screen) -> (f32, f32) {
        let reference = |r: Reference| match r {
            Reference::ScreenWidth => screen.width,
            Reference::ScreenHeight => screen.height,
        };
        match size.kind {
            SizeType::Dp => (size.width_dp * screen.density, size.height_dp * screen.density),
            SizeType::Percentage => (
                reference(size.width_reference) * size.width_percentage as f32 / 10000.0,
                reference(size.height_reference) * size.height_percentage as f32 / 10000.0,
            ),
            SizeType::WrapContent => self
                .measured
                .get(uuid)
                .copied()
                .unwrap_or((50.0 * screen.density, 50.0 * screen.density)),
        }
    }

    fn place(position: Position, (w, h): (f32, f32), screen: Screen) -> Rect {
        let p = |v: i32| (v as f32 / 10000.0).clamp(0.0, 1.0);
        Rect {
            x: (screen.width - w) * p(position.x),
            y: (screen.height - h) * p(position.y),
            w,
            h,
        }
    }

    fn button_rect(&self, (l, i): Id, screen: Screen) -> Rect {
        let b = &self.layout.layers[l].normal_buttons[i];
        Self::place(b.position, self.size(&b.button_size, &b.uuid, screen), screen)
    }

    fn stick_rect(&self, (l, i): Id, screen: Screen) -> Rect {
        let j = &self.layout.layers[l].joystick_buttons[i];
        let side = match j.size_type {
            SizeType::Dp => j.size_dp.max(20.0) * screen.density,
            SizeType::Percentage => screen.height * j.size_percentage.max(2000) as f32 / 10000.0,
            SizeType::WrapContent => 200.0 * screen.density,
        };
        Self::place(j.position, (side, side), screen)
    }

    fn stick_shape(&self, (l, i): Id, dark: bool) -> JoystickStyleConfig {
        let j = &self.layout.layers[l].joystick_buttons[i];
        self.layout
            .joystick_style(j.joystick_style_id.as_deref())
            .map_or_else(default_joystick_style, |s| s.config(dark).clone())
    }

    /// The touchable buttons from the top: the first layer's last button first.
    fn visible_buttons(&self, in_game: bool) -> Vec<Id> {
        let mut ids = Vec::new();
        for (l, layer) in self.layout.layers.iter().enumerate() {
            if !self.layer_shown(l, in_game) {
                continue;
            }
            for (i, b) in layer.normal_buttons.iter().enumerate().rev() {
                if self.shown(b.visibility_type, in_game) {
                    ids.push((l, i));
                }
            }
        }
        ids
    }

    fn button(&self, (l, i): Id) -> &Button {
        &self.layout.layers[l].normal_buttons[i]
    }

    fn send(&mut self, events: &[ClickEvent], pressed: bool) {
        for e in events {
            match e.kind {
                EventType::Key => self.events.push(Event::Key {
                    name: e.key.clone(),
                    pressed,
                }),
                EventType::LauncherEvent => self.events.push(Event::Launcher {
                    name: e.key.clone(),
                    pressed,
                }),
                EventType::SendText if pressed && !e.key.is_empty() => {
                    self.events.push(Event::SendText(e.key.clone()))
                }
                _ => {}
            }
        }
    }

    fn press(&mut self, (l, i): Id) {
        let behavior = Behavior::of(self.button((l, i)));
        match behavior {
            Behavior::Toggle => self.pressed[l][i] = !self.pressed[l][i],
            _ => {
                if self.pressed[l][i] {
                    return;
                }
                self.pressed[l][i] = true;
            }
        }
        let events = self.button((l, i)).click_events.clone();
        for e in &events {
            if let Some(layer) = self.layout.layers.iter().position(|x| x.uuid == e.key) {
                match e.kind {
                    EventType::SwitchLayer => self.hidden[layer] = !self.hidden[layer],
                    EventType::ShowLayer => self.hidden[layer] = false,
                    EventType::HideLayer => self.hidden[layer] = true,
                    _ => {}
                }
            }
        }
        let pressed = self.pressed[l][i];
        self.send(&events, pressed);
    }

    fn release(&mut self, (l, i): Id) {
        if Behavior::of(self.button((l, i))) == Behavior::Toggle || !self.pressed[l][i] {
            return;
        }
        self.pressed[l][i] = false;
        let events = self.button((l, i)).click_events.clone();
        self.send(&events, false);
    }

    /// The buttons a finger at `at` reaches: Zalith's deep touch detection,
    /// where a button both swipeable and penetrable blocks what is under it.
    fn targets(&self, at: (f32, f32), in_game: bool, screen: Screen) -> Option<Id> {
        let hits: Vec<Id> = self
            .visible_buttons(in_game)
            .into_iter()
            .filter(|&id| self.button_rect(id, screen).contains(at))
            .collect();
        let first = *hits.first()?;
        let b = self.button(first);
        (!(b.is_swipple && b.is_penetrable)).then_some(first)
    }

    /// Routes a finger that is down at `at` (landing or moving) through the buttons.
    fn route(&mut self, id: u64, at: (f32, f32), in_game: bool, screen: Screen) -> Claim {
        // Swipeable buttons the finger left let go of it.
        let active = self.fingers.get(&id).map(|f| f.active.clone()).unwrap_or_default();
        if !active.is_empty() {
            let mut kept = Vec::new();
            let mut removed = false;
            for w in &active {
                if Behavior::of(self.button(*w)) != Behavior::Swipe {
                    kept.push(*w);
                } else if self.button_rect(*w, screen).contains(at) {
                    kept.push(*w);
                    self.press(*w);
                } else {
                    self.release(*w);
                    removed = true;
                }
            }
            let finger = self.fingers.entry(id).or_default();
            if removed {
                finger.active = kept.clone();
            }
            if kept.is_empty() {
                finger.swipe_chain = true;
            } else if finger.swipe_chain {
                finger.swipe_chain = false;
            }
        }
        let Some(target) = self.targets(at, in_game, screen) else {
            return Claim::None;
        };
        let behavior = Behavior::of(self.button(target));
        let finger = self.fingers.entry(id).or_default();
        if finger.swipe_chain && behavior != Behavior::Swipe {
            return Claim::None;
        }
        if finger.active.is_empty() {
            finger.active.push(target);
            self.press(target);
            return if self.button(target).is_penetrable {
                Claim::MoveOnly
            } else {
                Claim::Taken
            };
        }
        let blocked = finger
            .active
            .iter()
            .any(|w| Behavior::of(&self.layout.layers[w.0].normal_buttons[w.1]) == Behavior::Toggle);
        if !finger.active.contains(&target) && behavior == Behavior::Swipe && !blocked {
            finger.active.push(target);
            self.press(target);
        }
        Claim::Taken
    }

    /// A finger landed at `at`. `in_game` is whether the world has the pointer.
    pub fn down(&mut self, id: u64, at: (f32, f32), in_game: bool, screen: Screen) -> Claim {
        let claim = self.route(id, at, in_game, screen);
        if claim == Claim::Taken {
            return claim;
        }
        // A joystick under the finger takes it, even under a penetrable button.
        for l in 0..self.layout.layers.len() {
            if !self.layer_shown(l, in_game) {
                continue;
            }
            for i in 0..self.layout.layers[l].joystick_buttons.len() {
                if !self.layout.layers[l].joystick_buttons[i]
                    .visibility_type
                    .shown(in_game)
                    || self.sticks[l][i].finger.is_some()
                {
                    continue;
                }
                let r = self.stick_rect((l, i), screen);
                let shape = self.stick_shape((l, i), screen.dark);
                let radius = r.w * shape.background_shape as f32 / 100.0;
                if !in_rounded(r, radius, at) {
                    continue;
                }
                let local = (at.0 - r.x, at.1 - r.y);
                let stick = &mut self.sticks[l][i];
                stick.finger = Some(id);
                stick.last = local;
                stick.locked = false;
                if self.layout.layers[l].joystick_buttons[i].trigger_mode == TriggerMode::Touch {
                    self.move_stick((l, i), local, screen);
                }
                return Claim::Taken;
            }
        }
        if claim == Claim::None {
            self.fingers.remove(&id);
        }
        claim
    }

    /// The finger moved: joysticks follow it, buttons swipe.
    pub fn moved(&mut self, id: u64, at: (f32, f32), in_game: bool, screen: Screen) {
        if let Some(stick) = self.stick_of(id) {
            let r = self.stick_rect(stick, screen);
            let local = (at.0 - r.x, at.1 - r.y);
            let s = &mut self.sticks[stick.0][stick.1];
            s.last = local;
            s.locked = false;
            self.move_stick(stick, local, screen);
            return;
        }
        if self.fingers.contains_key(&id) {
            self.route(id, at, in_game, screen);
        }
    }

    pub fn up(&mut self, id: u64, screen: Screen) {
        if let Some(stick) = self.stick_of(id) {
            let r = self.stick_rect(stick, screen);
            let centre = (r.w / 2.0, r.h / 2.0);
            let s = &mut self.sticks[stick.0][stick.1];
            s.finger = None;
            if s.can_lock {
                // Let go past the top: forward stays held, locked.
                s.locked = true;
                s.can_lock = false;
                self.move_stick(stick, (centre.0, 0.0), screen);
                self.sticks[stick.0][stick.1].locked = true;
            } else {
                s.locked = false;
                self.move_stick(stick, centre, screen);
            }
            return;
        }
        if let Some(finger) = self.fingers.remove(&id) {
            for w in finger.active {
                self.release(w);
            }
        }
    }

    /// Everything pressed lets go (the controls hide, the app leaves the screen).
    pub fn release_all(&mut self) {
        for (_, finger) in std::mem::take(&mut self.fingers) {
            for w in finger.active {
                self.release(w);
            }
        }
        for l in 0..self.sticks.len() {
            for i in 0..self.sticks[l].len() {
                let s = self.sticks[l][i];
                let j = self.layout.layers[l].joystick_buttons[i].clone();
                if let Some(d) = s.direction {
                    let events = j.direction_events.get(&d).cloned().unwrap_or_default();
                    self.send(&events, false);
                }
                if s.can_lock || s.locked {
                    let events = j.lock_events.clone();
                    self.send(&events, false);
                }
                self.sticks[l][i] = Stick::default();
            }
        }
    }

    fn stick_of(&self, id: u64) -> Option<Id> {
        self.sticks.iter().enumerate().find_map(|(l, row)| {
            row.iter()
                .position(|s| s.finger == Some(id))
                .map(|i| (l, i))
        })
    }

    /// `updateJoystickState`: the knob, the direction's keys and the lock.
    fn move_stick(&mut self, (l, i): Id, local: (f32, f32), screen: Screen) {
        let r = self.stick_rect((l, i), screen);
        let shape = self.stick_shape((l, i), screen.dark);
        let centre = (r.w / 2.0, r.h / 2.0);
        let radius = r.w.min(r.h) / 2.0;
        let j = self.layout.layers[l].joystick_buttons[i].clone();
        let corner = r.w * shape.background_shape as f32 / 100.0;
        let local_rect = Rect {
            x: 0.0,
            y: 0.0,
            w: r.w,
            h: r.h,
        };
        let clamped = if in_rounded(local_rect, corner, local) {
            local
        } else {
            // Bisect along the line to the centre for the region's edge.
            let (mut low, mut high, mut result) = (0.0f32, 1.0f32, centre);
            for _ in 0..10 {
                let mid = (low + high) / 2.0;
                let p = (
                    centre.0 + (local.0 - centre.0) * mid,
                    centre.1 + (local.1 - centre.1) * mid,
                );
                if in_rounded(local_rect, corner, p) {
                    result = p;
                    low = mid;
                } else {
                    high = mid;
                }
            }
            result
        };
        let stick = self.sticks[l][i];
        let direction = direction(clamped, centre, radius * j.dead_zone_ratio);
        if direction != stick.direction {
            if let Some(old) = stick.direction {
                let events = j.direction_events.get(&old).cloned().unwrap_or_default();
                self.send(&events, false);
            }
            if let Some(new) = direction {
                let events = j.direction_events.get(&new).cloned().unwrap_or_default();
                self.send(&events, true);
            }
        }
        let can_lock =
            j.can_lock && direction == Some(Direction::North) && stick.last.1 < -radius * j.lock_threshold;
        if can_lock && !stick.can_lock {
            self.send(&j.lock_events, true);
        } else if !can_lock && stick.can_lock {
            self.send(&j.lock_events, false);
        }
        let s = &mut self.sticks[l][i];
        s.knob = (clamped.0 - centre.0, clamped.1 - centre.1);
        s.direction = direction;
        s.can_lock = can_lock;
    }

    /// Draws every visible control over the HUD; `opacity` is the player's, 0..1.
    pub fn draw(&mut self, batch: &mut HudBatch, atlas: &HudAtlas, in_game: bool, screen: Screen, opacity: f32) {
        let mut ui = Ui::new(atlas, batch, (0.0, 0.0));
        // The last layer is at the bottom.
        for l in (0..self.layout.layers.len()).rev() {
            if !self.layer_shown(l, in_game) {
                continue;
            }
            let layer = self.layout.layers[l].clone();
            for t in &layer.text_boxes {
                if self.shown(t.visibility_type, in_game) {
                    let size = self.size(&t.button_size, &t.uuid, screen);
                    self.widget(
                        &mut ui,
                        Widget {
                            uuid: &t.uuid,
                            text: t.text.translate(&self.language),
                            position: t.position,
                            size,
                            wrap: t.button_size.kind == SizeType::WrapContent,
                            style: t.button_style.as_deref(),
                            alignment: t.text_alignment,
                            bold: t.text_bold,
                            underline: t.text_underline,
                            pressed: false,
                        },
                        screen,
                        opacity,
                    );
                }
            }
            for (i, b) in layer.normal_buttons.iter().enumerate() {
                if self.shown(b.visibility_type, in_game) {
                    let size = self.size(&b.button_size, &b.uuid, screen);
                    self.widget(
                        &mut ui,
                        Widget {
                            uuid: &b.uuid,
                            text: b.text.translate(&self.language),
                            position: b.position,
                            size,
                            wrap: b.button_size.kind == SizeType::WrapContent,
                            style: b.button_style.as_deref(),
                            alignment: b.text_alignment,
                            bold: b.text_bold,
                            underline: b.text_underline,
                            pressed: self.pressed[l][i],
                        },
                        screen,
                        opacity,
                    );
                }
            }
            for (i, j) in layer.joystick_buttons.iter().enumerate() {
                if self.shown(j.visibility_type, in_game) {
                    self.draw_stick(&mut ui, (l, i), screen, opacity);
                }
            }
        }
        ui.finish();
    }

    fn widget(&mut self, ui: &mut Ui, w: Widget, screen: Screen, opacity: f32) {
        let default = default_style();
        let style = self
            .layout
            .style(w.style)
            .map_or(&default, |s| s.config(screen.dark))
            .clone();
        let p = w.pressed;
        let alpha = if p { style.pressed_alpha } else { style.alpha } * opacity;
        let sp = if p { style.pressed_font_size } else { style.font_size }
            .map_or(TEXT_SP, |s| s as f32);
        let face = if w.bold { Face::SemiBold } else { Face::Regular };
        let d = ui.density;
        // Text sizes are in the Ui's dp; the layout's are physical pixels.
        let size = sp * screen.density / d;
        let line = (size * 1.1 * 4.0).round() / 4.0;
        let lines: Vec<&str> = w.text.split('\n').collect();
        let widths: Vec<f32> = lines.iter().map(|t| ui.measure(face, size, t)).collect();
        let text_w = widths.iter().copied().fold(0.0, f32::max);
        let text_h = line * lines.len() as f32;
        let (bw, bh) = if w.wrap {
            let measured = (text_w * d, text_h * d);
            self.measured.insert(w.uuid.to_owned(), measured);
            measured
        } else {
            w.size
        };
        let r = Self::place(w.position, (bw, bh), screen);
        let (x0, y0, x1, y1) = (r.x.round(), r.y.round(), (r.x + r.w).round(), (r.y + r.h).round());
        let shape = if p { style.pressed_border_radius } else { style.border_radius };
        let radii = [shape.top_start, shape.top_end, shape.bottom_end, shape.bottom_start]
            .map(|r| r * screen.density);
        let colour = |c: Argb| with_alpha(c, alpha);
        let bg = if p { style.pressed_background_color } else { style.background_color };
        ui.rounded_px(x0, y0, x1, y1, radii, colour(bg));
        let border = if p { style.pressed_border_width } else { style.border_width };
        if border > 0 {
            let bc = if p { style.pressed_border_color } else { style.border_color };
            ui.border_px(x0, y0, x1, y1, radii, border as f32 * screen.density, colour(bc));
        }
        let content = colour(if p { style.pressed_content_color } else { style.content_color });
        let top = (y0 + y1) / 2.0 / d - text_h / 2.0;
        let left = (x0 + x1) / 2.0 / d - text_w / 2.0;
        for (n, (text, width)) in lines.iter().zip(&widths).enumerate() {
            let x = match w.alignment {
                TextAlignment::Left => left,
                TextAlignment::Center => left + (text_w - width) / 2.0,
                TextAlignment::Right => left + text_w - width,
            };
            let y = top + n as f32 * line;
            ui.text_line(face, size, line, x, y, content, text);
            if w.underline {
                let under = ((y + line * 0.92) * d).round();
                ui.rounded_px((x * d).round(), under, ((x + width) * d).round(), under + 1.0, [0.0; 4], content);
            }
        }
    }

    fn draw_stick(&self, ui: &mut Ui, id: Id, screen: Screen, opacity: f32) {
        let r = self.stick_rect(id, screen);
        let shape = self.stick_shape(id, screen.dark);
        let s = self.sticks[id.0][id.1];
        let alpha = shape.alpha * opacity;
        let colour = |c: Argb| with_alpha(c, alpha);
        let side = r.w.min(r.h);
        let (x0, y0, x1, y1) = (r.x.round(), r.y.round(), (r.x + r.w).round(), (r.y + r.h).round());
        let corner = side * shape.background_shape as f32 / 100.0;
        ui.rounded_px(x0, y0, x1, y1, [corner; 4], colour(shape.background_color));
        let border = side * (shape.border_width_ratio as f32 / 100.0).clamp(0.0, 0.5);
        if border >= 1.0 {
            ui.border_px(x0, y0, x1, y1, [corner; 4], border, colour(shape.border_color));
        }
        let knob = side * shape.joystick_size.clamp(0.0, 1.0);
        let (cx, cy) = (r.x + r.w / 2.0 + s.knob.0, r.y + r.h / 2.0 + s.knob.1);
        let knob_corner = knob * shape.joystick_shape as f32 / 100.0;
        let knob_colour = if s.locked {
            shape.joystick_locked_color
        } else if s.can_lock {
            shape.joystick_can_lock_color
        } else {
            shape.joystick_color
        };
        ui.rounded_px(
            (cx - knob / 2.0).round(),
            (cy - knob / 2.0).round(),
            (cx + knob / 2.0).round(),
            (cy + knob / 2.0).round(),
            [knob_corner; 4],
            colour(knob_colour),
        );
        if s.locked {
            let (mx, my) = (r.x + r.w / 2.0, r.y);
            ui.rounded_px(
                (mx - 4.0).round(),
                (my - 4.0).round(),
                (mx + 4.0).round(),
                (my + 4.0).round(),
                [4.0; 4],
                colour(shape.lock_mark_color),
            );
        }
    }
}

/// The menu's mouse pointer: an arrow `size` physical pixels tall with its
/// tip at `at`, white with a dark edge.
pub fn draw_pointer(batch: &mut HudBatch, atlas: &HudAtlas, at: (f32, f32), size: f32) {
    // The arrow in units of its height: the head, then the tail.
    const HEAD: [(f32, f32); 3] = [(0.0, 0.0), (0.0, 0.8), (0.56, 0.6)];
    const TAIL: [(f32, f32); 4] = [(0.2, 0.62), (0.33, 0.57), (0.52, 0.98), (0.39, 1.03)];
    let s = batch.scale;
    let point = |(x, y): (f32, f32), grow: f32, (ox, oy): (f32, f32)| {
        [(at.0 + x * size + ox) / s, (at.1 + y * size + oy) / s, grow]
    };
    let edge = (size / 16.0).max(1.0);
    for (offset, colour) in [
        ((-edge, 0.0), 0xc000_0000),
        ((edge, 0.0), 0xc000_0000),
        ((0.0, -edge), 0xc000_0000),
        ((0.0, edge), 0xc000_0000),
        ((0.0, 0.0), 0xffff_ffff),
    ] {
        let p = |v| {
            let [x, y, _] = point(v, 0.0, offset);
            [x, y]
        };
        batch.fill_quad(atlas, [p(HEAD[0]), p(HEAD[1]), p(HEAD[2]), p(HEAD[2])], colour);
        batch.fill_quad(atlas, [p(TAIL[0]), p(TAIL[1]), p(TAIL[2]), p(TAIL[3])], colour);
    }
}

struct Widget<'a> {
    uuid: &'a str,
    text: &'a str,
    position: Position,
    size: (f32, f32),
    wrap: bool,
    style: Option<&'a str>,
    alignment: TextAlignment,
    bold: bool,
    underline: bool,
    pressed: bool,
}

/// An ARGB colour times `alpha`, in the HUD's encoding (ABGR).
fn with_alpha(c: Argb, alpha: f32) -> u32 {
    let a = ((c.0 >> 24) as f32 * alpha.clamp(0.0, 1.0)).round() as u32;
    let (r, g, b) = ((c.0 >> 16) & 0xff, (c.0 >> 8) & 0xff, c.0 & 0xff);
    a << 24 | b << 16 | g << 8 | r
}

/// Whether `p` is inside `r` with corners of `radius`.
fn in_rounded(r: Rect, radius: f32, p: (f32, f32)) -> bool {
    if !r.contains(p) {
        return false;
    }
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    let cx = p.0.clamp(r.x + radius, r.x + r.w - radius);
    let cy = p.1.clamp(r.y + radius, r.y + r.h - radius);
    (p.0 - cx).hypot(p.1 - cy) <= radius + 0.5
}

/// `calculateDirection`: eight ways around the centre, none inside the dead zone.
fn direction(at: (f32, f32), centre: (f32, f32), dead_zone: f32) -> Option<Direction> {
    let (dx, dy) = (at.0 - centre.0, at.1 - centre.1);
    if (dx == 0.0 && dy == 0.0) || dx.hypot(dy) < dead_zone {
        return None;
    }
    let angle = dy.atan2(dx).to_degrees();
    Some(match angle {
        a if (-22.5..22.5).contains(&a) => Direction::East,
        a if (22.5..67.5).contains(&a) => Direction::SouthEast,
        a if (67.5..112.5).contains(&a) => Direction::South,
        a if (112.5..157.5).contains(&a) => Direction::SouthWest,
        a if !(-157.5..157.5).contains(&a) => Direction::West,
        a if (-157.5..-112.5).contains(&a) => Direction::NorthWest,
        a if (-112.5..-67.5).contains(&a) => Direction::North,
        _ => Direction::NorthEast,
    })
}

#[cfg(test)]
mod tests;
