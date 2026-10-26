//! The control layout editor, after Zalith Launcher 2's: every layer shows,
//! a widget drags where it goes, and a tap selects it for its panel (label,
//! size, keys, how it behaves, where it shows, its style). The toolbar adds
//! buttons and joysticks to the current layer, adds layers, saves and leaves.
use super::widgets::{self, Hits, MUTED, PLATE, TEXT};
use super::*;
use crate::ui_theme::Face;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected {
    Button(usize, usize),
    Stick(usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    AddButton,
    AddStick,
    AddLayer,
    NextLayer,
    HideLayer,
    Save,
    Exit,
    Deselect,
    EditLabel,
    Width(i32),
    Height(i32),
    Keys,
    Toggle,
    Swipe,
    Penetrable,
    Visibility,
    Style,
    Duplicate,
    Delete,
    Lock,
    Trigger,
    DeadZone(i32),
    Pick(ClickEvent),
    PickDone,
    PickScroll(f32),
}

/// What the game does after a touch on the editor.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    None,
    /// Show or hide the soft keyboard for a label.
    Keyboard(bool),
    /// Write the layout and leave.
    Save,
    /// Leave without saving.
    Exit,
}

struct Drag {
    finger: u64,
    widget: Selected,
    start: (f32, f32),
    origin: (f32, f32),
    moved: bool,
}

pub struct Editor {
    pub controls: Controls,
    selected: Option<Selected>,
    layer: usize,
    drag: Option<Drag>,
    /// A press on a toolbar or panel button, until the finger lifts.
    pending: Option<(u64, Action)>,
    hits: Hits<Action>,
    picking: bool,
    pick_scroll: f32,
    editing_label: bool,
    pub dirty: bool,
}

/// Every event a button can send, with its label in the key picker.
fn catalogue(layout: &Layout) -> Vec<(ClickEvent, String)> {
    let mut all = Vec::new();
    let key = |name: &str| ClickEvent::key(name);
    let launcher = |name: &str| ClickEvent {
        kind: EventType::LauncherEvent,
        key: name.to_owned(),
    };
    for (event, label) in [
        (launcher("GLFW_MOUSE_BUTTON_LEFT"), "Left click"),
        (launcher("GLFW_MOUSE_BUTTON_RIGHT"), "Right click"),
        (launcher("GLFW_MOUSE_BUTTON_MIDDLE"), "Middle click"),
        (launcher("launcher.event.switch_ime"), "Keyboard"),
        (launcher("launcher.event.switch_menu"), "Menu"),
        (launcher("launcher.event.scroll_up"), "Scroll up"),
        (launcher("launcher.event.scroll_down"), "Scroll down"),
    ] {
        all.push((event, label.to_owned()));
    }
    let mut names: Vec<String> = ('A'..='Z')
        .chain('0'..='9')
        .map(|c| format!("GLFW_KEY_{c}"))
        .collect();
    names.extend((1..=12).map(|n| format!("GLFW_KEY_F{n}")));
    for n in [
        "SPACE", "LEFT_SHIFT", "LEFT_CONTROL", "LEFT_ALT", "TAB", "ESCAPE", "ENTER", "BACKSPACE",
        "UP", "DOWN", "LEFT", "RIGHT", "SLASH", "PERIOD", "COMMA", "MINUS", "EQUAL", "SEMICOLON",
        "APOSTROPHE", "LEFT_BRACKET", "RIGHT_BRACKET", "BACKSLASH", "GRAVE_ACCENT", "DELETE",
        "INSERT", "HOME", "END", "PAGE_UP", "PAGE_DOWN", "RIGHT_SHIFT", "RIGHT_CONTROL",
        "RIGHT_ALT", "CAPS_LOCK",
    ] {
        names.push(format!("GLFW_KEY_{n}"));
    }
    for name in names {
        let label = name.trim_start_matches("GLFW_KEY_").replace('_', " ");
        all.push((key(&name), label));
    }
    for layer in &layout.layers {
        let name = if layer.name.is_empty() { "layer" } else { &layer.name };
        all.push((
            ClickEvent {
                kind: EventType::SwitchLayer,
                key: layer.uuid.clone(),
            },
            format!("Show or hide {name}"),
        ));
    }
    all
}

fn new_uuid() -> String {
    let mut bytes = [0u8; 9];
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (seed >> (i * 8) ^ seed >> (i * 5 + 3)) as u8;
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Editor {
    pub fn new(mut controls: Controls) -> Self {
        controls.release_all();
        controls.take_events();
        controls.show_all = true;
        Self {
            controls,
            selected: None,
            layer: 0,
            drag: None,
            pending: None,
            hits: Hits::new(),
            picking: false,
            pick_scroll: 0.0,
            editing_label: false,
            dirty: false,
        }
    }

    /// The edited layout, for saving.
    pub fn layout(&self) -> &Layout {
        &self.controls.layout
    }

    /// The controls to play with after editing.
    pub fn into_controls(self) -> Controls {
        let mut c = Controls::new(self.controls.layout);
        c.language = self.controls.language;
        c
    }

    /// Where the toolbar's Add button was drawn.
    #[cfg(test)]
    pub(super) fn add_button_at(&self) -> Option<(f32, f32)> {
        self.hits.find(&Action::AddButton)
    }

    /// Text typed while a label is edited.
    pub fn type_text(&mut self, text: &str) {
        if let (true, Some(Selected::Button(l, i))) = (self.editing_label, self.selected) {
            self.controls.layout.layers[l].normal_buttons[i]
                .text
                .default
                .push_str(text);
            self.dirty = true;
        }
    }

    pub fn backspace(&mut self) {
        if let (true, Some(Selected::Button(l, i))) = (self.editing_label, self.selected) {
            self.controls.layout.layers[l].normal_buttons[i].text.default.pop();
            self.dirty = true;
        }
    }

    /// Enter or the back gesture ends a label's editing; `false` when nothing was being edited.
    pub fn finish_typing(&mut self) -> bool {
        std::mem::replace(&mut self.editing_label, false)
    }

    fn rect_of(&self, w: Selected, screen: Screen) -> Rect {
        match w {
            Selected::Button(l, i) => self.controls.button_rect((l, i), screen),
            Selected::Stick(l, i) => self.controls.stick_rect((l, i), screen),
        }
    }

    /// The widget under `at`, the topmost first.
    fn widget_at(&self, at: (f32, f32), screen: Screen) -> Option<Selected> {
        for (l, layer) in self.controls.layout.layers.iter().enumerate() {
            for i in (0..layer.joystick_buttons.len()).rev() {
                if self.controls.stick_rect((l, i), screen).contains(at) {
                    return Some(Selected::Stick(l, i));
                }
            }
            for i in (0..layer.normal_buttons.len()).rev() {
                if self.controls.button_rect((l, i), screen).contains(at) {
                    return Some(Selected::Button(l, i));
                }
            }
        }
        None
    }

    pub fn down(&mut self, finger: u64, at: (f32, f32), screen: Screen) {
        if let Some(action) = self.hits.at(at) {
            self.pending = Some((finger, action));
            return;
        }
        if self.picking {
            return;
        }
        let Some(widget) = self.widget_at(at, screen) else {
            return;
        };
        let r = self.rect_of(widget, screen);
        self.drag = Some(Drag {
            finger,
            widget,
            start: at,
            origin: (r.x, r.y),
            moved: false,
        });
    }

    pub fn moved(&mut self, finger: u64, at: (f32, f32), screen: Screen) {
        if let Some((f, Action::PickScroll(_))) = &self.pending
            && *f == finger
        {
            return;
        }
        let Some(drag) = self.drag.as_mut().filter(|d| d.finger == finger) else {
            return;
        };
        let (dx, dy) = (at.0 - drag.start.0, at.1 - drag.start.1);
        if !drag.moved && dx.hypot(dy) < 8.0 * screen.density {
            return;
        }
        drag.moved = true;
        let (widget, origin) = (drag.widget, drag.origin);
        let r = self.rect_of(widget, screen);
        let to = |v: f32, room: f32| {
            if room <= 0.0 {
                0
            } else {
                ((v / room) * 10000.0).round().clamp(0.0, 10000.0) as i32
            }
        };
        let position = Position {
            x: to(origin.0 + dx, screen.width - r.w),
            y: to(origin.1 + dy, screen.height - r.h),
        };
        match widget {
            Selected::Button(l, i) => self.controls.layout.layers[l].normal_buttons[i].position = position,
            Selected::Stick(l, i) => self.controls.layout.layers[l].joystick_buttons[i].position = position,
        }
        self.dirty = true;
    }

    pub fn up(&mut self, finger: u64, at: (f32, f32), screen: Screen) -> Outcome {
        if let Some(drag) = self.drag.take_if(|d| d.finger == finger) {
            if !drag.moved {
                self.select(Some(drag.widget));
            }
            return Outcome::None;
        }
        let Some((f, action)) = self.pending.take() else {
            return Outcome::None;
        };
        if f != finger || self.hits.at(at).as_ref() != Some(&action) {
            return Outcome::None;
        }
        self.act(action, screen)
    }

    fn select(&mut self, widget: Option<Selected>) {
        self.selected = widget;
        self.picking = false;
        if let Some(Selected::Button(l, _) | Selected::Stick(l, _)) = widget {
            self.layer = l;
        }
    }

    fn button_mut(&mut self) -> Option<&mut Button> {
        match self.selected? {
            Selected::Button(l, i) => Some(&mut self.controls.layout.layers[l].normal_buttons[i]),
            Selected::Stick(..) => None,
        }
    }

    fn stick_mut(&mut self) -> Option<&mut Joystick> {
        match self.selected? {
            Selected::Stick(l, i) => Some(&mut self.controls.layout.layers[l].joystick_buttons[i]),
            Selected::Button(..) => None,
        }
    }

    /// The layout changed shape: the runtime state follows it.
    fn rebuild(&mut self) {
        let mut c = Controls::new(self.controls.layout.clone());
        c.language = std::mem::take(&mut self.controls.language);
        c.show_all = true;
        self.controls = c;
        self.dirty = true;
    }

    fn act(&mut self, action: Action, screen: Screen) -> Outcome {
        let layers = self.controls.layout.layers.len();
        match action {
            Action::AddButton | Action::AddStick | Action::NextLayer | Action::HideLayer
                if layers == 0 =>
            {
                return self.act(Action::AddLayer, screen);
            }
            Action::AddLayer => {
                self.controls.layout.layers.push(Layer {
                    name: format!("layer{}", layers + 1),
                    uuid: new_uuid(),
                    hide: false,
                    hide_when_mouse: true,
                    hide_when_gamepad: true,
                    visibility_type: Visibility::Always,
                    normal_buttons: Vec::new(),
                    text_boxes: Vec::new(),
                    joystick_buttons: Vec::new(),
                });
                self.layer = layers;
                self.rebuild();
            }
            Action::AddButton => {
                let size = (50.0 * screen.density / screen.height * 10000.0) as i32;
                let layer = &mut self.controls.layout.layers[self.layer];
                layer.normal_buttons.push(Button {
                    text: Text {
                        default: "Button".into(),
                        match_queue: Vec::new(),
                    },
                    uuid: new_uuid(),
                    position: Position { x: 5000, y: 5000 },
                    button_size: Size {
                        kind: SizeType::Percentage,
                        width_dp: 50.0,
                        height_dp: 50.0,
                        width_percentage: size.clamp(100, 10000),
                        height_percentage: size.clamp(100, 10000),
                        width_reference: Reference::ScreenHeight,
                        height_reference: Reference::ScreenHeight,
                    },
                    button_style: None,
                    text_alignment: TextAlignment::Left,
                    text_bold: false,
                    text_italic: false,
                    text_underline: false,
                    visibility_type: Visibility::Always,
                    click_events: Vec::new(),
                    is_swipple: false,
                    is_penetrable: false,
                    is_toggleable: false,
                });
                let i = layer.normal_buttons.len() - 1;
                self.rebuild();
                self.select(Some(Selected::Button(self.layer, i)));
            }
            Action::AddStick => {
                let layer = &mut self.controls.layout.layers[self.layer];
                layer.joystick_buttons.push(Joystick {
                    uuid: new_uuid(),
                    position: Position { x: 1000, y: 8000 },
                    size_type: SizeType::Percentage,
                    size_dp: 200.0,
                    size_percentage: 3417,
                    visibility_type: Visibility::InGame,
                    joystick_style_id: None,
                    dead_zone_ratio: 0.5,
                    lock_threshold: 0.3,
                    can_lock: true,
                    trigger_mode: TriggerMode::Drag,
                    direction_events: default_direction_events(),
                    lock_events: vec![ClickEvent::key("GLFW_KEY_LEFT_CONTROL")],
                });
                let i = layer.joystick_buttons.len() - 1;
                self.rebuild();
                self.select(Some(Selected::Stick(self.layer, i)));
            }
            Action::NextLayer => {
                self.layer = (self.layer + 1) % layers;
                self.selected = None;
            }
            Action::HideLayer => {
                let layer = &mut self.controls.layout.layers[self.layer];
                layer.hide = !layer.hide;
                self.rebuild();
            }
            Action::Save => return Outcome::Save,
            Action::Exit => return Outcome::Exit,
            Action::Deselect => {
                self.select(None);
                if self.finish_typing() {
                    return Outcome::Keyboard(false);
                }
            }
            Action::EditLabel => {
                self.editing_label = !self.editing_label;
                return Outcome::Keyboard(self.editing_label);
            }
            Action::Width(d) | Action::Height(d) => {
                let width = matches!(action, Action::Width(_));
                if let Some(b) = self.button_mut() {
                    let s = &mut b.button_size;
                    if s.kind != SizeType::Percentage {
                        s.kind = SizeType::Percentage;
                    }
                    let v = if width { &mut s.width_percentage } else { &mut s.height_percentage };
                    *v = (*v + d).clamp(100, 10000);
                } else if let Some(j) = self.stick_mut() {
                    j.size_type = SizeType::Percentage;
                    j.size_percentage = (j.size_percentage + d).clamp(2000, 10000);
                }
                self.dirty = true;
            }
            Action::Keys => {
                self.picking = !self.picking;
                self.pick_scroll = 0.0;
            }
            Action::Pick(event) => {
                if let Some(b) = self.button_mut() {
                    if let Some(at) = b.click_events.iter().position(|e| *e == event) {
                        b.click_events.remove(at);
                    } else {
                        b.click_events.push(event);
                    }
                } else if let Some(j) = self.stick_mut() {
                    if let Some(at) = j.lock_events.iter().position(|e| *e == event) {
                        j.lock_events.remove(at);
                    } else {
                        j.lock_events.push(event);
                    }
                }
                self.dirty = true;
            }
            Action::PickDone => self.picking = false,
            Action::PickScroll(d) => self.pick_scroll = (self.pick_scroll + d).max(0.0),
            Action::Toggle => {
                if let Some(b) = self.button_mut() {
                    b.is_toggleable = !b.is_toggleable;
                }
                self.rebuild();
            }
            Action::Swipe => {
                if let Some(b) = self.button_mut() {
                    b.is_swipple = !b.is_swipple;
                }
                self.rebuild();
            }
            Action::Penetrable => {
                if let Some(b) = self.button_mut() {
                    b.is_penetrable = !b.is_penetrable;
                }
                self.rebuild();
            }
            Action::Visibility => {
                let next = |v: Visibility| match v {
                    Visibility::Always => Visibility::InGame,
                    Visibility::InGame => Visibility::InMenu,
                    Visibility::InMenu => Visibility::Always,
                };
                if let Some(b) = self.button_mut() {
                    b.visibility_type = next(b.visibility_type);
                } else if let Some(j) = self.stick_mut() {
                    j.visibility_type = next(j.visibility_type);
                }
                self.dirty = true;
            }
            Action::Style => {
                let styles: Vec<String> = self.controls.layout.styles.iter().map(|s| s.uuid.clone()).collect();
                if let Some(b) = self.button_mut() {
                    let now = styles.iter().position(|s| Some(s) == b.button_style.as_ref());
                    b.button_style = match now {
                        None => styles.first().cloned(),
                        Some(i) => styles.get(i + 1).cloned(),
                    };
                }
                self.dirty = true;
            }
            Action::Lock => {
                if let Some(j) = self.stick_mut() {
                    j.can_lock = !j.can_lock;
                }
                self.dirty = true;
            }
            Action::Trigger => {
                if let Some(j) = self.stick_mut() {
                    j.trigger_mode = match j.trigger_mode {
                        TriggerMode::Drag => TriggerMode::Touch,
                        TriggerMode::Touch => TriggerMode::Drag,
                    };
                }
                self.dirty = true;
            }
            Action::DeadZone(d) => {
                if let Some(j) = self.stick_mut() {
                    j.dead_zone_ratio = (j.dead_zone_ratio + d as f32 / 100.0).clamp(0.0, 0.9);
                }
                self.dirty = true;
            }
            Action::Duplicate => {
                match self.selected {
                    Some(Selected::Button(l, i)) => {
                        let mut b = self.controls.layout.layers[l].normal_buttons[i].clone();
                        b.uuid = new_uuid();
                        b.position = Position { x: 5000, y: 5000 };
                        self.controls.layout.layers[l].normal_buttons.push(b);
                        let n = self.controls.layout.layers[l].normal_buttons.len() - 1;
                        self.rebuild();
                        self.select(Some(Selected::Button(l, n)));
                    }
                    Some(Selected::Stick(l, i)) => {
                        let mut j = self.controls.layout.layers[l].joystick_buttons[i].clone();
                        j.uuid = new_uuid();
                        j.position = Position { x: 5000, y: 5000 };
                        self.controls.layout.layers[l].joystick_buttons.push(j);
                        let n = self.controls.layout.layers[l].joystick_buttons.len() - 1;
                        self.rebuild();
                        self.select(Some(Selected::Stick(l, n)));
                    }
                    None => {}
                }
            }
            Action::Delete => {
                match self.selected.take() {
                    Some(Selected::Button(l, i)) => {
                        self.controls.layout.layers[l].normal_buttons.remove(i);
                    }
                    Some(Selected::Stick(l, i)) => {
                        self.controls.layout.layers[l].joystick_buttons.remove(i);
                    }
                    None => {}
                }
                self.rebuild();
                if self.finish_typing() {
                    return Outcome::Keyboard(false);
                }
            }
        }
        Outcome::None
    }

    pub fn draw(&mut self, batch: &mut HudBatch, atlas: &HudAtlas, screen: Screen) {
        self.controls.draw(batch, atlas, true, screen, 1.0);
        let mut ui = Ui::new(atlas, batch, (0.0, 0.0));
        self.hits.clear();
        let d = screen.density;
        // The selection, outlined in the accent.
        if let Some(w) = self.selected {
            let r = self.rect_of(w, screen);
            ui.border_px(
                (r.x - 2.0 * d).round(),
                (r.y - 2.0 * d).round(),
                (r.x + r.w + 2.0 * d).round(),
                (r.y + r.h + 2.0 * d).round(),
                [4.0 * d; 4],
                2.0 * d,
                widgets::ACCENT,
            );
        }
        self.toolbar(&mut ui, screen);
        if self.picking {
            self.picker(&mut ui, screen);
        } else if self.selected.is_some() {
            self.panel(&mut ui, screen);
        }
        ui.finish();
    }

    fn toolbar(&mut self, ui: &mut Ui, screen: Screen) {
        let d = screen.density;
        let h = 34.0 * d;
        let layer = self
            .controls
            .layout
            .layers
            .get(self.layer)
            .map_or_else(|| "none".to_owned(), |l| {
                let name = if l.name.is_empty() { "layer" } else { &l.name };
                if l.hide { format!("{name} (hidden)") } else { name.to_owned() }
            });
        let items: [(String, bool, Action); 7] = [
            ("Add button".into(), false, Action::AddButton),
            ("Add joystick".into(), false, Action::AddStick),
            (format!("Layer: {layer}"), false, Action::NextLayer),
            ("Show or hide layer".into(), false, Action::HideLayer),
            ("New layer".into(), false, Action::AddLayer),
            ("Save".into(), true, Action::Save),
            ("Exit".into(), false, Action::Exit),
        ];
        let widths: Vec<f32> = items
            .iter()
            .map(|(l, _, _)| widgets::text_width(ui, 13.0, Face::Medium, l) + h)
            .collect();
        let gap = 6.0 * d;
        let total: f32 = widths.iter().sum::<f32>() + gap * (items.len() - 1) as f32;
        let mut x = ((screen.width - total) / 2.0).max(8.0 * d);
        let y = 8.0 * d;
        widgets::plate(
            ui,
            Rect {
                x: x - 6.0 * d,
                y: y - 6.0 * d,
                w: total + 12.0 * d,
                h: h + 12.0 * d,
            },
            (h + 12.0 * d) / 2.0,
            PLATE,
        );
        for ((label, primary, action), w) in items.into_iter().zip(widths) {
            widgets::button(ui, &mut self.hits, Rect { x, y, w, h }, &label, primary, action);
            x += w + gap;
        }
    }

    fn panel(&mut self, ui: &mut Ui, screen: Screen) {
        let d = screen.density;
        let w = 300.0 * d;
        let r = Rect {
            x: screen.width - w - 12.0 * d,
            y: 60.0 * d,
            w,
            h: screen.height - 72.0 * d,
        };
        widgets::plate(ui, r, 12.0 * d, PLATE);
        let row = 34.0 * d;
        let mut y = r.y + 12.0 * d;
        let x = r.x + 14.0 * d;
        let right = r.x + r.w - 14.0 * d;
        let small = 26.0 * d;
        let line = |ui: &mut Ui, y: &mut f32, name: &str, value: &str| {
            widgets::text(ui, x, *y + (row - 13.0 * 1.45 * d) / 2.0, 13.0, Face::Medium, TEXT, name);
            let vw = widgets::text_width(ui, 12.0, Face::Regular, value);
            widgets::text(ui, (x + 110.0 * d).min(right - vw), *y + (row - 12.0 * 1.45 * d) / 2.0, 12.0, Face::Regular, MUTED, value);
            *y += row;
        };
        let events_label = |events: &[ClickEvent], all: &[(ClickEvent, String)]| {
            let names: Vec<String> = events
                .iter()
                .map(|e| {
                    all.iter()
                        .find(|(c, _)| c == e)
                        .map_or_else(|| e.key.clone(), |(_, l)| l.clone())
                })
                .collect();
            if names.is_empty() { "none".to_owned() } else { names.join(", ") }
        };
        let all = catalogue(&self.controls.layout);
        match self.selected {
            Some(Selected::Button(l, i)) => {
                let b = self.controls.layout.layers[l].normal_buttons[i].clone();
                let label = if self.editing_label { format!("{}|", b.text.default) } else { b.text.default.clone() };
                line(ui, &mut y, "Label", &label);
                widgets::button_at(ui, &mut self.hits, x, y, small, if self.editing_label { "Done" } else { "Edit label" }, self.editing_label, Action::EditLabel);
                y += row;
                let percent = |v: i32| format!("{:.1}%", v as f32 / 100.0);
                line(ui, &mut y, "Width", &percent(b.button_size.width_percentage));
                let mut bx = x;
                for (label, a) in [("−", Action::Width(-50)), ("+", Action::Width(50)), ("Height −", Action::Height(-50)), ("Height +", Action::Height(50))] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, label, false, a) + 6.0 * d;
                }
                y += row;
                line(ui, &mut y, "Height", &percent(b.button_size.height_percentage));
                line(ui, &mut y, "Sends", &events_label(&b.click_events, &all));
                widgets::button_at(ui, &mut self.hits, x, y, small, "Choose keys", false, Action::Keys);
                y += row;
                let yes = |v: bool| if v { "on" } else { "off" };
                let mut bx = x;
                for (label, a) in [
                    (format!("Toggle: {}", yes(b.is_toggleable)), Action::Toggle),
                    (format!("Swipe: {}", yes(b.is_swipple)), Action::Swipe),
                ] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, &label, false, a) + 6.0 * d;
                }
                y += row;
                let mut bx = x;
                for (label, a) in [
                    (format!("Look through: {}", yes(b.is_penetrable)), Action::Penetrable),
                    (format!("Shows: {}", visibility_name(b.visibility_type)), Action::Visibility),
                ] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, &label, false, a) + 6.0 * d;
                }
                y += row;
                let style = self
                    .controls
                    .layout
                    .style(b.button_style.as_deref())
                    .map_or("Default", |s| s.name.as_str())
                    .to_owned();
                widgets::button_at(ui, &mut self.hits, x, y, small, &format!("Style: {style}"), false, Action::Style);
                y += row;
            }
            Some(Selected::Stick(l, i)) => {
                let j = self.controls.layout.layers[l].joystick_buttons[i].clone();
                line(ui, &mut y, "Joystick", &format!("{:.1}% of the height", j.size_percentage as f32 / 100.0));
                let mut bx = x;
                for (label, a) in [("Smaller", Action::Width(-100)), ("Larger", Action::Width(100))] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, label, false, a) + 6.0 * d;
                }
                y += row;
                line(ui, &mut y, "Dead zone", &format!("{:.0}%", j.dead_zone_ratio * 100.0));
                let mut bx = x;
                for (label, a) in [("−", Action::DeadZone(-5)), ("+", Action::DeadZone(5))] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, label, false, a) + 6.0 * d;
                }
                y += row;
                let mut bx = x;
                for (label, a) in [
                    (format!("Forward lock: {}", if j.can_lock { "on" } else { "off" }), Action::Lock),
                    (
                        format!("Starts: {}", if j.trigger_mode == TriggerMode::Drag { "on drag" } else { "on touch" }),
                        Action::Trigger,
                    ),
                ] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, &label, false, a) + 6.0 * d;
                }
                y += row;
                line(ui, &mut y, "Lock sends", &events_label(&j.lock_events, &all));
                let mut bx = x;
                for (label, a) in [
                    ("Choose keys".to_owned(), Action::Keys),
                    (format!("Shows: {}", visibility_name(j.visibility_type)), Action::Visibility),
                ] {
                    bx = widgets::button_at(ui, &mut self.hits, bx, y, small, &label, false, a) + 6.0 * d;
                }
                y += row;
            }
            None => {}
        }
        let bottom = r.y + r.h - small - 12.0 * d;
        let mut bx = x;
        for (label, a) in [("Duplicate", Action::Duplicate), ("Delete", Action::Delete), ("Close", Action::Deselect)] {
            bx = widgets::button_at(ui, &mut self.hits, bx, bottom.max(y), small, label, false, a) + 6.0 * d;
        }
    }

    fn picker(&mut self, ui: &mut Ui, screen: Screen) {
        let d = screen.density;
        let r = Rect {
            x: 24.0 * d,
            y: 60.0 * d,
            w: screen.width - 48.0 * d,
            h: screen.height - 72.0 * d,
        };
        widgets::plate(ui, r, 12.0 * d, PLATE);
        let chosen: Vec<ClickEvent> = match self.selected {
            Some(Selected::Button(l, i)) => self.controls.layout.layers[l].normal_buttons[i].click_events.clone(),
            Some(Selected::Stick(l, i)) => self.controls.layout.layers[l].joystick_buttons[i].lock_events.clone(),
            None => Vec::new(),
        };
        let h = 30.0 * d;
        let gap = 6.0 * d;
        let mut x = r.x + 14.0 * d;
        let mut y = r.y + 14.0 * d - self.pick_scroll;
        let right = r.x + r.w - 14.0 * d;
        let top = r.y + 8.0 * d;
        let bottom = r.y + r.h - h - 20.0 * d;
        for (event, label) in catalogue(&self.controls.layout) {
            let w = widgets::text_width(ui, 13.0, Face::Medium, &label) + h;
            if x + w > right {
                x = r.x + 14.0 * d;
                y += h + gap;
            }
            if y >= top && y + h <= bottom {
                let on = chosen.contains(&event);
                widgets::button(ui, &mut self.hits, Rect { x, y, w, h }, &label, on, Action::Pick(event));
            }
            x += w + gap;
        }
        let mut bx = r.x + 14.0 * d;
        for (label, a, primary) in [
            ("Done", Action::PickDone, true),
            ("Up", Action::PickScroll(-(h + gap) * 3.0), false),
            ("Down", Action::PickScroll((h + gap) * 3.0), false),
        ] {
            bx = widgets::button_at(ui, &mut self.hits, bx, r.y + r.h - h - 12.0 * d, h, label, primary, a) + gap;
        }
    }
}

fn visibility_name(v: Visibility) -> &'static str {
    match v {
        Visibility::Always => "always",
        Visibility::InGame => "in the world",
        Visibility::InMenu => "in menus",
    }
}
