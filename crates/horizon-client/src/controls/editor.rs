//! The control layout editor, after Zalith Launcher 2's. Every layer shows.
//! A widget drags where it goes, its edges snapping to the screen's and to
//! other widgets' with a guide line, and the handle on the selected one's
//! corner resizes it. A tap selects a widget for its dialog, in Zalith's
//! tabs: Basic (position, size, where it shows), Text, Style and Events for
//! a button; Basic, Joystick, Directions and Style for a joystick. The
//! toolbar adds buttons, text and joysticks, opens the layers, saves and
//! leaves.
use super::widgets::{self, Hits, MUTED, PLATE, TEXT};
use super::*;
use crate::ui_theme::Face;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected {
    Button(usize, usize),
    Text(usize, usize),
    Stick(usize, usize),
}

impl Selected {
    fn layer(self) -> usize {
        match self {
            Selected::Button(l, _) | Selected::Text(l, _) | Selected::Stick(l, _) => l,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Basic,
    Text,
    Style,
    Events,
    Joystick,
    Directions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slide {
    X,
    Y,
    W,
    H,
    Side,
    DeadZone,
    LockThreshold,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Typing {
    Label,
    SendText,
    LayerName(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PickFor {
    Events,
    Lock,
    Direction(Direction),
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    AddButton,
    AddText,
    AddStick,
    Layers,
    Save,
    Exit,
    Close,
    Tab(Tab),
    Slide(Slide),
    SizeType,
    Visibility,
    Align,
    Bold,
    Italic,
    Underline,
    Type(Typing),
    Style(Option<String>),
    StickStyle(Option<String>),
    Pick(PickFor),
    PickEvent(ClickEvent),
    PickDone,
    PickScroll(f32),
    Toggle,
    Swipe,
    Penetrable,
    CanLock,
    Trigger,
    Duplicate,
    Delete,
    LayerSelect(usize),
    LayerHide(usize),
    LayerShows(usize),
    LayerUp(usize),
    LayerDelete(usize),
    NewLayer,
}

/// What the game does after a touch on the editor.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    None,
    /// Show or hide the soft keyboard for typing.
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
    origin: Rect,
    moved: bool,
}

struct Resize {
    finger: u64,
    start: (f32, f32),
    origin: Rect,
}

pub struct Editor {
    pub controls: Controls,
    selected: Option<Selected>,
    tab: Tab,
    /// The layer new widgets go to.
    layer: usize,
    drag: Option<Drag>,
    resize: Option<Resize>,
    /// A slider a finger drags, and its track's area.
    sliding: Option<(u64, Slide, Rect)>,
    /// A press on a dialog's button, until the finger lifts.
    pending: Option<(u64, Action)>,
    hits: Hits<Action>,
    picking: Option<PickFor>,
    pick_scroll: f32,
    layers_open: bool,
    typing: Option<Typing>,
    /// Where the dragged widget snapped: vertical lines (x) and horizontal (y).
    guides: Vec<(bool, f32)>,
    pub dirty: bool,
}

/// Every event a button can send, with its label in the picker.
fn catalogue(layout: &Layout) -> Vec<(ClickEvent, String)> {
    let mut all = Vec::new();
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
        (
            launcher("launcher.event.scroll_up.single"),
            "Scroll up once",
        ),
        (
            launcher("launcher.event.scroll_down.single"),
            "Scroll down once",
        ),
    ] {
        all.push((event, label.to_owned()));
    }
    let mut names: Vec<String> = ('A'..='Z')
        .chain('0'..='9')
        .map(|c| format!("GLFW_KEY_{c}"))
        .collect();
    names.extend((1..=12).map(|n| format!("GLFW_KEY_F{n}")));
    for n in [
        "SPACE",
        "LEFT_SHIFT",
        "LEFT_CONTROL",
        "LEFT_ALT",
        "TAB",
        "ESCAPE",
        "ENTER",
        "BACKSPACE",
        "UP",
        "DOWN",
        "LEFT",
        "RIGHT",
        "SLASH",
        "PERIOD",
        "COMMA",
        "MINUS",
        "EQUAL",
        "SEMICOLON",
        "APOSTROPHE",
        "LEFT_BRACKET",
        "RIGHT_BRACKET",
        "BACKSLASH",
        "GRAVE_ACCENT",
        "DELETE",
        "INSERT",
        "HOME",
        "END",
        "PAGE_UP",
        "PAGE_DOWN",
        "RIGHT_SHIFT",
        "RIGHT_CONTROL",
        "RIGHT_ALT",
        "CAPS_LOCK",
    ] {
        names.push(format!("GLFW_KEY_{n}"));
    }
    for name in names {
        let label = name.trim_start_matches("GLFW_KEY_").replace('_', " ");
        all.push((ClickEvent::key(&name), label));
    }
    for layer in &layout.layers {
        let name = if layer.name.is_empty() {
            "layer"
        } else {
            &layer.name
        };
        for (kind, verb) in [
            (EventType::SwitchLayer, "Show or hide"),
            (EventType::ShowLayer, "Show"),
            (EventType::HideLayer, "Hide"),
        ] {
            all.push((
                ClickEvent {
                    kind,
                    key: layer.uuid.clone(),
                },
                format!("{verb} {name}"),
            ));
        }
    }
    all
}

fn events_label(events: &[ClickEvent], all: &[(ClickEvent, String)]) -> String {
    let names: Vec<String> = events
        .iter()
        .filter(|e| e.kind != EventType::SendText)
        .map(|e| {
            all.iter()
                .find(|(c, _)| c == e)
                .map_or_else(|| e.key.clone(), |(_, l)| l.clone())
        })
        .collect();
    if names.is_empty() {
        "nothing".to_owned()
    } else {
        names.join(", ")
    }
}

fn new_uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ COUNT
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    format!("{:012x}", seed & 0xffff_ffff_ffff)
}

fn visibility_name(v: Visibility) -> &'static str {
    match v {
        Visibility::Always => "always",
        Visibility::InGame => "in the world",
        Visibility::InMenu => "in menus",
    }
}

fn next_visibility(v: Visibility) -> Visibility {
    match v {
        Visibility::Always => Visibility::InGame,
        Visibility::InGame => Visibility::InMenu,
        Visibility::InMenu => Visibility::Always,
    }
}

fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "Up",
        Direction::NorthEast => "Up right",
        Direction::East => "Right",
        Direction::SouthEast => "Down right",
        Direction::South => "Down",
        Direction::SouthWest => "Down left",
        Direction::West => "Left",
        Direction::NorthWest => "Up left",
        Direction::None => "None",
    }
}

const DIRECTIONS: [Direction; 8] = [
    Direction::NorthWest,
    Direction::North,
    Direction::NorthEast,
    Direction::West,
    Direction::East,
    Direction::SouthWest,
    Direction::South,
    Direction::SouthEast,
];

impl Editor {
    pub fn new(mut controls: Controls) -> Self {
        controls.release_all();
        controls.take_events();
        controls.show_all = true;
        Self {
            controls,
            selected: None,
            tab: Tab::Basic,
            layer: 0,
            drag: None,
            resize: None,
            sliding: None,
            pending: None,
            hits: Hits::new(),
            picking: None,
            pick_scroll: 0.0,
            layers_open: false,
            typing: None,
            guides: Vec::new(),
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
        c.size = self.controls.size;
        c
    }

    /// Where the toolbar's Add button was drawn.
    #[cfg(test)]
    pub(super) fn add_button_at(&self) -> Option<(f32, f32)> {
        self.hits.find(&Action::AddButton)
    }

    fn typed_mut(&mut self) -> Option<&mut String> {
        match (self.typing?, self.selected) {
            (Typing::Label, Some(Selected::Button(l, i))) => Some(
                &mut self.controls.layout.layers[l].normal_buttons[i]
                    .text
                    .default,
            ),
            (Typing::Label, Some(Selected::Text(l, i))) => {
                Some(&mut self.controls.layout.layers[l].text_boxes[i].text.default)
            }
            (Typing::SendText, Some(Selected::Button(l, i))) => {
                let events = &mut self.controls.layout.layers[l].normal_buttons[i].click_events;
                let at = match events.iter().position(|e| e.kind == EventType::SendText) {
                    Some(at) => at,
                    None => {
                        events.push(ClickEvent {
                            kind: EventType::SendText,
                            key: String::new(),
                        });
                        events.len() - 1
                    }
                };
                Some(&mut events[at].key)
            }
            (Typing::LayerName(l), _) => {
                self.controls.layout.layers.get_mut(l).map(|l| &mut l.name)
            }
            _ => None,
        }
    }

    /// Text typed into the field being edited.
    pub fn type_text(&mut self, text: &str) {
        if let Some(field) = self.typed_mut() {
            field.push_str(text);
            self.dirty = true;
        }
    }

    pub fn backspace(&mut self) {
        if let Some(field) = self.typed_mut() {
            field.pop();
            self.dirty = true;
        }
    }

    /// Enter or the back gesture ends typing; `false` when nothing was being typed.
    pub fn finish_typing(&mut self) -> bool {
        if let Some(Selected::Button(l, i)) = self.selected {
            // An emptied text to send is no event.
            self.controls.layout.layers[l].normal_buttons[i]
                .click_events
                .retain(|e| e.kind != EventType::SendText || !e.key.is_empty());
        }
        self.typing.take().is_some()
    }

    fn rect_of(&self, w: Selected, screen: Screen) -> Rect {
        match w {
            Selected::Button(l, i) => self.controls.button_rect((l, i), screen),
            Selected::Text(l, i) => {
                let t = &self.controls.layout.layers[l].text_boxes[i];
                Controls::place(
                    t.position,
                    self.controls.size(&t.button_size, &t.uuid, screen),
                    screen,
                )
            }
            Selected::Stick(l, i) => self.controls.stick_rect((l, i), screen),
        }
    }

    fn all_widgets(&self) -> Vec<Selected> {
        let mut all = Vec::new();
        for (l, layer) in self.controls.layout.layers.iter().enumerate() {
            all.extend((0..layer.text_boxes.len()).map(|i| Selected::Text(l, i)));
            all.extend((0..layer.normal_buttons.len()).map(|i| Selected::Button(l, i)));
            all.extend((0..layer.joystick_buttons.len()).map(|i| Selected::Stick(l, i)));
        }
        all
    }

    /// The widget under `at`, the topmost (first layer, last drawn) first.
    fn widget_at(&self, at: (f32, f32), screen: Screen) -> Option<Selected> {
        for (l, layer) in self.controls.layout.layers.iter().enumerate() {
            let in_layer = (0..layer.joystick_buttons.len())
                .rev()
                .map(|i| Selected::Stick(l, i))
                .chain(
                    (0..layer.normal_buttons.len())
                        .rev()
                        .map(|i| Selected::Button(l, i)),
                )
                .chain(
                    (0..layer.text_boxes.len())
                        .rev()
                        .map(|i| Selected::Text(l, i)),
                );
            for w in in_layer {
                if self.rect_of(w, screen).contains(at) {
                    return Some(w);
                }
            }
        }
        None
    }

    fn handle(&self, screen: Screen) -> Option<(f32, f32, f32)> {
        let w = self.selected?;
        let r = self.rect_of(w, screen);
        Some((r.x + r.w, r.y + r.h, 11.0 * screen.density))
    }

    fn dialog_open(&self) -> bool {
        self.picking.is_some() || self.layers_open
    }

    pub fn down(&mut self, finger: u64, at: (f32, f32), screen: Screen) {
        if let Some((action, rect)) = self.hits.hit(at) {
            if let Action::Slide(kind) = action {
                self.sliding = Some((finger, kind, rect));
                self.slide(
                    kind,
                    widgets::slider_fraction(rect, at.0, screen.density),
                    screen,
                );
            } else {
                self.pending = Some((finger, action));
            }
            return;
        }
        if self.dialog_open() {
            return;
        }
        if let Some((x, y, r)) = self.handle(screen)
            && (at.0 - x).hypot(at.1 - y) <= r * 2.0
        {
            let origin = self.rect_of(self.selected.unwrap(), screen);
            self.resize = Some(Resize {
                finger,
                start: at,
                origin,
            });
            return;
        }
        let Some(widget) = self.widget_at(at, screen) else {
            return;
        };
        let origin = self.rect_of(widget, screen);
        self.drag = Some(Drag {
            finger,
            widget,
            start: at,
            origin,
            moved: false,
        });
    }

    pub fn moved(&mut self, finger: u64, at: (f32, f32), screen: Screen) {
        if let Some((f, kind, rect)) = self.sliding
            && f == finger
        {
            self.slide(
                kind,
                widgets::slider_fraction(rect, at.0, screen.density),
                screen,
            );
            return;
        }
        if let Some(resize) = self.resize.as_ref().filter(|r| r.finger == finger) {
            let (dx, dy) = (at.0 - resize.start.0, at.1 - resize.start.1);
            let origin = resize.origin;
            self.resize_to(origin.w + dx, origin.h + dy, screen);
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
        let (x, y) = self.snap(
            widget,
            origin.x + dx,
            origin.y + dy,
            origin.w,
            origin.h,
            screen,
        );
        self.place_at(widget, x, y, screen);
    }

    pub fn up(&mut self, finger: u64, at: (f32, f32), screen: Screen) -> Outcome {
        if self.sliding.is_some_and(|(f, _, _)| f == finger) {
            self.sliding = None;
            return Outcome::None;
        }
        if self.resize.take_if(|r| r.finger == finger).is_some() {
            return Outcome::None;
        }
        if let Some(drag) = self.drag.take_if(|d| d.finger == finger) {
            self.guides.clear();
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

    /// Edges near the screen's or another widget's snap to them.
    fn snap(
        &mut self,
        widget: Selected,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        screen: Screen,
    ) -> (f32, f32) {
        let threshold = 6.0 * screen.density;
        let mut xs = vec![0.0, screen.width, screen.width / 2.0];
        let mut ys = vec![0.0, screen.height, screen.height / 2.0];
        for other in self.all_widgets() {
            if other == widget {
                continue;
            }
            let r = self.rect_of(other, screen);
            xs.extend([r.x, r.x + r.w, r.x + r.w / 2.0]);
            ys.extend([r.y, r.y + r.h, r.y + r.h / 2.0]);
        }
        self.guides.clear();
        let best = |edges: [f32; 3], lines: &[f32]| -> Option<(f32, f32)> {
            let mut best: Option<(f32, f32)> = None;
            for edge in edges {
                for &line in lines {
                    let d = line - edge;
                    if d.abs() <= threshold && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                        best = Some((d, line));
                    }
                }
            }
            best
        };
        let (mut x, mut y) = (
            x.clamp(0.0, (screen.width - w).max(0.0)),
            y.clamp(0.0, (screen.height - h).max(0.0)),
        );
        if let Some((d, line)) = best([x, x + w, x + w / 2.0], &xs) {
            x += d;
            self.guides.push((true, line));
        }
        if let Some((d, line)) = best([y, y + h, y + h / 2.0], &ys) {
            y += d;
            self.guides.push((false, line));
        }
        (x, y)
    }

    /// Moves a widget so its top left corner is at (x, y) physical pixels.
    fn place_at(&mut self, widget: Selected, x: f32, y: f32, screen: Screen) {
        let r = self.rect_of(widget, screen);
        let to = |v: f32, room: f32| {
            if room <= 0.0 {
                0
            } else {
                ((v / room) * 10000.0).round().clamp(0.0, 10000.0) as i32
            }
        };
        let p = Position {
            x: to(x, screen.width - r.w),
            y: to(y, screen.height - r.h),
        };
        let layers = &mut self.controls.layout.layers;
        match widget {
            Selected::Button(l, i) => layers[l].normal_buttons[i].position = p,
            Selected::Text(l, i) => layers[l].text_boxes[i].position = p,
            Selected::Stick(l, i) => layers[l].joystick_buttons[i].position = p,
        }
        self.dirty = true;
    }

    fn size_mut(&mut self) -> Option<&mut Size> {
        match self.selected? {
            Selected::Button(l, i) => {
                Some(&mut self.controls.layout.layers[l].normal_buttons[i].button_size)
            }
            Selected::Text(l, i) => {
                Some(&mut self.controls.layout.layers[l].text_boxes[i].button_size)
            }
            Selected::Stick(..) => None,
        }
    }

    /// The selected widget at `w` x `h` physical pixels, in its own size type.
    fn resize_to(&mut self, w: f32, h: f32, screen: Screen) {
        let k = screen.density;
        if let Some(Selected::Stick(l, i)) = self.selected {
            let j = &mut self.controls.layout.layers[l].joystick_buttons[i];
            let side = w.max(h);
            match j.size_type {
                SizeType::Dp | SizeType::WrapContent => {
                    j.size_type = SizeType::Dp;
                    j.size_dp = (side / k).clamp(20.0, screen.height / k);
                }
                SizeType::Percentage => {
                    j.size_percentage = ((side / screen.height) * 10000.0)
                        .round()
                        .clamp(2000.0, 10000.0) as i32;
                }
            }
            self.dirty = true;
            return;
        }
        let Some(s) = self.size_mut() else {
            return;
        };
        let reference = |r: Reference| match r {
            Reference::ScreenWidth => screen.width,
            Reference::ScreenHeight => screen.height,
        };
        if s.kind == SizeType::WrapContent {
            s.kind = SizeType::Dp;
        }
        match s.kind {
            SizeType::Dp => {
                s.width_dp = (w / k).clamp(5.0, screen.width / k);
                s.height_dp = (h / k).clamp(5.0, screen.height / k);
            }
            _ => {
                let pct = |v: f32, r: f32| ((v / r) * 10000.0).round().clamp(100.0, 10000.0) as i32;
                s.width_percentage = pct(w, reference(s.width_reference));
                s.height_percentage = pct(h, reference(s.height_reference));
            }
        }
        self.dirty = true;
    }

    fn slide(&mut self, kind: Slide, f: f32, screen: Screen) {
        let Some(w) = self.selected else {
            return;
        };
        let k = screen.density;
        let r = self.rect_of(w, screen);
        match kind {
            Slide::X => {
                let room = screen.width - r.w;
                self.place_at(w, f * room, r.y, screen);
            }
            Slide::Y => {
                let room = screen.height - r.h;
                self.place_at(w, r.x, f * room, screen);
            }
            Slide::W | Slide::H => {
                let (max_w, max_h) = (screen.width.min(400.0 * k), screen.height.min(400.0 * k));
                let min = 5.0 * k;
                if kind == Slide::W {
                    self.resize_to(min + f * (max_w - min), r.h, screen);
                } else {
                    self.resize_to(r.w, min + f * (max_h - min), screen);
                }
            }
            Slide::Side => {
                let (min, max) = (20.0 * k, screen.height);
                let side = min + f * (max - min);
                self.resize_to(side, side, screen);
            }
            Slide::DeadZone | Slide::LockThreshold => {
                if let Selected::Stick(l, i) = w {
                    let j = &mut self.controls.layout.layers[l].joystick_buttons[i];
                    if kind == Slide::DeadZone {
                        j.dead_zone_ratio = (f * 0.9 * 100.0).round() / 100.0;
                    } else {
                        j.lock_threshold = (f * 100.0).round() / 100.0;
                    }
                    self.dirty = true;
                }
            }
        }
    }

    fn select(&mut self, widget: Option<Selected>) {
        if widget != self.selected {
            self.tab = Tab::Basic;
            self.typing = None;
        }
        self.selected = widget;
        self.picking = None;
        if let Some(w) = widget {
            self.layer = w.layer();
        }
    }

    fn button_mut(&mut self) -> Option<&mut Button> {
        match self.selected? {
            Selected::Button(l, i) => Some(&mut self.controls.layout.layers[l].normal_buttons[i]),
            _ => None,
        }
    }

    fn stick_mut(&mut self) -> Option<&mut Joystick> {
        match self.selected? {
            Selected::Stick(l, i) => Some(&mut self.controls.layout.layers[l].joystick_buttons[i]),
            _ => None,
        }
    }

    fn text_style_mut(&mut self) -> Option<(&mut TextAlignment, &mut bool, &mut bool, &mut bool)> {
        match self.selected? {
            Selected::Button(l, i) => {
                let b = &mut self.controls.layout.layers[l].normal_buttons[i];
                Some((
                    &mut b.text_alignment,
                    &mut b.text_bold,
                    &mut b.text_italic,
                    &mut b.text_underline,
                ))
            }
            Selected::Text(l, i) => {
                let t = &mut self.controls.layout.layers[l].text_boxes[i];
                Some((
                    &mut t.text_alignment,
                    &mut t.text_bold,
                    &mut t.text_italic,
                    &mut t.text_underline,
                ))
            }
            Selected::Stick(..) => None,
        }
    }

    /// The layout changed shape: the runtime state follows it.
    fn rebuild(&mut self) {
        let mut c = Controls::new(self.controls.layout.clone());
        c.language = std::mem::take(&mut self.controls.language);
        c.measured = std::mem::take(&mut self.controls.measured);
        c.size = self.controls.size;
        c.show_all = true;
        self.controls = c;
        self.dirty = true;
    }

    fn ensure_layer(&mut self) {
        if self.controls.layout.layers.is_empty() {
            self.act(
                Action::NewLayer,
                Screen {
                    width: 1.0,
                    height: 1.0,
                    density: 1.0,
                    dark: false,
                },
            );
        }
        self.layer = self.layer.min(self.controls.layout.layers.len() - 1);
    }

    fn default_size(screen: Screen) -> Size {
        let pct = ((50.0 * screen.density / screen.height) * 10000.0) as i32;
        Size {
            kind: SizeType::Percentage,
            width_dp: 50.0,
            height_dp: 50.0,
            width_percentage: pct.clamp(100, 10000),
            height_percentage: pct.clamp(100, 10000),
            width_reference: Reference::ScreenHeight,
            height_reference: Reference::ScreenHeight,
        }
    }

    fn act(&mut self, action: Action, screen: Screen) -> Outcome {
        match action {
            Action::AddButton => {
                self.ensure_layer();
                let layer = &mut self.controls.layout.layers[self.layer];
                layer.normal_buttons.push(Button {
                    text: Text {
                        default: "Button".into(),
                        match_queue: Vec::new(),
                    },
                    uuid: new_uuid(),
                    position: Position { x: 5000, y: 5000 },
                    button_size: Self::default_size(screen),
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
            Action::AddText => {
                self.ensure_layer();
                let layer = &mut self.controls.layout.layers[self.layer];
                let mut size = Self::default_size(screen);
                size.kind = SizeType::WrapContent;
                layer.text_boxes.push(TextBox {
                    text: Text {
                        default: "Text".into(),
                        match_queue: Vec::new(),
                    },
                    uuid: new_uuid(),
                    position: Position { x: 5000, y: 4000 },
                    button_size: size,
                    button_style: None,
                    text_alignment: TextAlignment::Left,
                    text_bold: false,
                    text_italic: false,
                    text_underline: false,
                    visibility_type: Visibility::Always,
                });
                let i = layer.text_boxes.len() - 1;
                self.rebuild();
                self.select(Some(Selected::Text(self.layer, i)));
            }
            Action::AddStick => {
                self.ensure_layer();
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
            Action::Layers => {
                self.layers_open = !self.layers_open;
                self.picking = None;
            }
            Action::Save => return Outcome::Save,
            Action::Exit => return Outcome::Exit,
            Action::Close => {
                let typing = self.finish_typing();
                if self.picking.is_some() {
                    self.picking = None;
                } else if self.layers_open {
                    self.layers_open = false;
                } else {
                    self.select(None);
                }
                if typing {
                    return Outcome::Keyboard(false);
                }
            }
            Action::Tab(tab) => {
                self.tab = tab;
                if self.finish_typing() {
                    return Outcome::Keyboard(false);
                }
            }
            Action::Slide(_) => {}
            Action::SizeType => {
                let (w, h) = self
                    .selected
                    .map(|s| {
                        let r = self.rect_of(s, screen);
                        (r.w, r.h)
                    })
                    .unwrap_or_default();
                if let Some(Selected::Stick(l, i)) = self.selected {
                    let j = &mut self.controls.layout.layers[l].joystick_buttons[i];
                    j.size_type = match j.size_type {
                        SizeType::Percentage => SizeType::Dp,
                        _ => SizeType::Percentage,
                    };
                } else if let Some(s) = self.size_mut() {
                    s.kind = match s.kind {
                        SizeType::Percentage => SizeType::Dp,
                        SizeType::Dp => SizeType::WrapContent,
                        SizeType::WrapContent => SizeType::Percentage,
                    };
                }
                // The widget keeps its size in the new type.
                let wrap = self
                    .selected
                    .and_then(|_| self.size_mut().map(|s| s.kind == SizeType::WrapContent))
                    .unwrap_or(false);
                if !wrap {
                    self.resize_to(w, h, screen);
                }
                self.dirty = true;
            }
            Action::Visibility => {
                let layers = &mut self.controls.layout.layers;
                match self.selected {
                    Some(Selected::Button(l, i)) => {
                        let b = &mut layers[l].normal_buttons[i];
                        b.visibility_type = next_visibility(b.visibility_type);
                    }
                    Some(Selected::Text(l, i)) => {
                        let t = &mut layers[l].text_boxes[i];
                        t.visibility_type = next_visibility(t.visibility_type);
                    }
                    Some(Selected::Stick(l, i)) => {
                        let j = &mut layers[l].joystick_buttons[i];
                        j.visibility_type = next_visibility(j.visibility_type);
                    }
                    None => {}
                }
                self.dirty = true;
            }
            Action::Align | Action::Bold | Action::Italic | Action::Underline => {
                if let Some((align, bold, italic, underline)) = self.text_style_mut() {
                    match action {
                        Action::Align => {
                            *align = match align {
                                TextAlignment::Left => TextAlignment::Center,
                                TextAlignment::Center => TextAlignment::Right,
                                TextAlignment::Right => TextAlignment::Left,
                            }
                        }
                        Action::Bold => *bold = !*bold,
                        Action::Italic => *italic = !*italic,
                        _ => *underline = !*underline,
                    }
                }
                self.dirty = true;
            }
            Action::Type(target) => {
                if self.typing == Some(target) {
                    self.finish_typing();
                    return Outcome::Keyboard(false);
                }
                self.finish_typing();
                self.typing = Some(target);
                return Outcome::Keyboard(true);
            }
            Action::Style(id) => {
                match self.selected {
                    Some(Selected::Button(l, i)) => {
                        self.controls.layout.layers[l].normal_buttons[i].button_style = id;
                    }
                    Some(Selected::Text(l, i)) => {
                        self.controls.layout.layers[l].text_boxes[i].button_style = id;
                    }
                    _ => {}
                }
                self.dirty = true;
            }
            Action::StickStyle(id) => {
                if let Some(j) = self.stick_mut() {
                    j.joystick_style_id = id;
                }
                self.dirty = true;
            }
            Action::Pick(target) => {
                self.picking = Some(target);
                self.pick_scroll = 0.0;
            }
            Action::PickEvent(event) => {
                let target = self.picking;
                let toggle = |events: &mut Vec<ClickEvent>| {
                    if let Some(at) = events.iter().position(|e| *e == event) {
                        events.remove(at);
                    } else {
                        events.push(event.clone());
                    }
                };
                match target {
                    Some(PickFor::Events) => {
                        if let Some(b) = self.button_mut() {
                            toggle(&mut b.click_events);
                        }
                    }
                    Some(PickFor::Lock) => {
                        if let Some(j) = self.stick_mut() {
                            toggle(&mut j.lock_events);
                        }
                    }
                    Some(PickFor::Direction(d)) => {
                        if let Some(j) = self.stick_mut() {
                            toggle(j.direction_events.entry(d).or_default());
                        }
                    }
                    None => {}
                }
                self.dirty = true;
            }
            Action::PickDone => self.picking = None,
            Action::PickScroll(d) => self.pick_scroll = (self.pick_scroll + d).max(0.0),
            Action::Toggle | Action::Swipe | Action::Penetrable => {
                if let Some(b) = self.button_mut() {
                    match action {
                        Action::Toggle => b.is_toggleable = !b.is_toggleable,
                        Action::Swipe => b.is_swipple = !b.is_swipple,
                        _ => b.is_penetrable = !b.is_penetrable,
                    }
                }
                self.rebuild();
            }
            Action::CanLock => {
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
            Action::Duplicate => {
                let layers = &mut self.controls.layout.layers;
                let copy = match self.selected {
                    Some(Selected::Button(l, i)) => {
                        let mut b = layers[l].normal_buttons[i].clone();
                        b.uuid = new_uuid();
                        b.position = Position { x: 5000, y: 5000 };
                        layers[l].normal_buttons.push(b);
                        Some(Selected::Button(l, layers[l].normal_buttons.len() - 1))
                    }
                    Some(Selected::Text(l, i)) => {
                        let mut t = layers[l].text_boxes[i].clone();
                        t.uuid = new_uuid();
                        t.position = Position { x: 5000, y: 4000 };
                        layers[l].text_boxes.push(t);
                        Some(Selected::Text(l, layers[l].text_boxes.len() - 1))
                    }
                    Some(Selected::Stick(l, i)) => {
                        let mut j = layers[l].joystick_buttons[i].clone();
                        j.uuid = new_uuid();
                        j.position = Position { x: 5000, y: 5000 };
                        layers[l].joystick_buttons.push(j);
                        Some(Selected::Stick(l, layers[l].joystick_buttons.len() - 1))
                    }
                    None => None,
                };
                self.rebuild();
                self.select(copy);
            }
            Action::Delete => {
                let layers = &mut self.controls.layout.layers;
                match self.selected.take() {
                    Some(Selected::Button(l, i)) => {
                        layers[l].normal_buttons.remove(i);
                    }
                    Some(Selected::Text(l, i)) => {
                        layers[l].text_boxes.remove(i);
                    }
                    Some(Selected::Stick(l, i)) => {
                        layers[l].joystick_buttons.remove(i);
                    }
                    None => {}
                }
                self.rebuild();
                if self.typing.take().is_some() {
                    return Outcome::Keyboard(false);
                }
            }
            Action::LayerSelect(l) => {
                self.layer = l;
                self.select(None);
            }
            Action::LayerHide(l) => {
                if let Some(layer) = self.controls.layout.layers.get_mut(l) {
                    layer.hide = !layer.hide;
                }
                self.rebuild();
            }
            Action::LayerShows(l) => {
                if let Some(layer) = self.controls.layout.layers.get_mut(l) {
                    layer.visibility_type = next_visibility(layer.visibility_type);
                }
                self.dirty = true;
            }
            Action::LayerUp(l) => {
                if l > 0 {
                    self.controls.layout.layers.swap(l, l - 1);
                    self.layer = l - 1;
                    self.selected = None;
                    self.rebuild();
                }
            }
            Action::LayerDelete(l) => {
                if l < self.controls.layout.layers.len() {
                    self.controls.layout.layers.remove(l);
                    self.selected = None;
                    self.layer = self
                        .layer
                        .min(self.controls.layout.layers.len().saturating_sub(1));
                    self.rebuild();
                }
            }
            Action::NewLayer => {
                let n = self.controls.layout.layers.len();
                self.controls.layout.layers.push(Layer {
                    name: format!("layer{}", n + 1),
                    uuid: new_uuid(),
                    hide: false,
                    hide_when_mouse: true,
                    hide_when_gamepad: true,
                    visibility_type: Visibility::Always,
                    normal_buttons: Vec::new(),
                    text_boxes: Vec::new(),
                    joystick_buttons: Vec::new(),
                });
                self.layer = n;
                self.rebuild();
            }
        }
        Outcome::None
    }

    pub fn draw(&mut self, batch: &mut HudBatch, atlas: &HudAtlas, screen: Screen) {
        self.controls.draw(batch, atlas, true, screen, 1.0);
        let mut ui = Ui::new(atlas, batch, (0.0, 0.0));
        self.hits.clear();
        let d = screen.density;
        for &(vertical, at) in &self.guides {
            let (x0, y0, x1, y1) = if vertical {
                (at - d / 2.0, 0.0, at + d / 2.0, screen.height)
            } else {
                (0.0, at - d / 2.0, screen.width, at + d / 2.0)
            };
            ui.rounded_px(
                x0.round(),
                y0.round(),
                x1.round().max(x0.round() + 1.0),
                y1.round().max(y0.round() + 1.0),
                [0.0; 4],
                widgets::ACCENT,
            );
        }
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
            if let Some((x, y, rad)) = self.handle(screen) {
                ui.rounded_px(
                    (x - rad).round(),
                    (y - rad).round(),
                    (x + rad).round(),
                    (y + rad).round(),
                    [rad; 4],
                    widgets::ACCENT,
                );
                ui.rounded_px(
                    (x - rad * 0.45).round(),
                    (y - rad * 0.45).round(),
                    (x + rad * 0.45).round(),
                    (y + rad * 0.45).round(),
                    [rad * 0.45; 4],
                    widgets::ON_ACCENT,
                );
            }
        }
        self.toolbar(&mut ui, screen);
        if let Some(target) = self.picking {
            self.picker(&mut ui, screen, target);
        } else if self.layers_open {
            self.layers(&mut ui, screen);
        } else if self.selected.is_some() {
            self.dialog(&mut ui, screen);
        }
        ui.finish();
    }

    fn toolbar(&mut self, ui: &mut Ui, screen: Screen) {
        let d = screen.density;
        let h = 32.0 * d;
        let layer = self.controls.layout.layers.get(self.layer).map_or_else(
            || "none".to_owned(),
            |l| {
                if l.name.is_empty() {
                    "layer".into()
                } else {
                    l.name.clone()
                }
            },
        );
        let items: [(String, bool, Action); 6] = [
            ("Add button".into(), false, Action::AddButton),
            ("Add text".into(), false, Action::AddText),
            ("Add joystick".into(), false, Action::AddStick),
            (
                format!("Layers ({layer})"),
                self.layers_open,
                Action::Layers,
            ),
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
            widgets::button(
                ui,
                &mut self.hits,
                Rect { x, y, w, h },
                &label,
                primary,
                action,
            );
            x += w + gap;
        }
    }

    /// The dialog's frame, on the side the selection is not, and its tabs.
    fn frame(&mut self, ui: &mut Ui, screen: Screen, tabs: &[(Tab, &str)]) -> Rect {
        let d = screen.density;
        let w = 360.0 * d;
        let selected_right = self
            .selected
            .map(|s| {
                let r = self.rect_of(s, screen);
                r.x + r.w / 2.0 > screen.width / 2.0
            })
            .unwrap_or(true);
        let x = if selected_right {
            12.0 * d
        } else {
            screen.width - w - 12.0 * d
        };
        let r = Rect {
            x,
            y: 56.0 * d,
            w,
            h: screen.height - 64.0 * d,
        };
        widgets::plate(ui, r, 12.0 * d, PLATE);
        let mut tx = r.x + 10.0 * d;
        let th = 28.0 * d;
        for &(tab, label) in tabs {
            tx = widgets::button_at(
                ui,
                &mut self.hits,
                tx,
                r.y + 10.0 * d,
                th,
                label,
                self.tab == tab,
                Action::Tab(tab),
            ) + 4.0 * d;
        }
        r
    }

    fn row(r: Rect, n: usize, d: f32) -> Rect {
        Rect {
            x: r.x + 14.0 * d,
            y: r.y + 50.0 * d + n as f32 * 34.0 * d,
            w: r.w - 28.0 * d,
            h: 30.0 * d,
        }
    }

    fn line(ui: &mut Ui, r: Rect, name: &str, value: &str) {
        let d = ui.density;
        widgets::text(
            ui,
            r.x,
            r.y + (r.h - 13.0 * 1.45 * d) / 2.0,
            13.0,
            Face::Medium,
            TEXT,
            name,
        );
        let max = r.w - 120.0 * d;
        let mut value = value.to_owned();
        while widgets::text_width(ui, 12.0, Face::Regular, &value) > max
            && value.chars().count() > 2
        {
            value.pop();
            value.pop();
            value.push('…');
        }
        widgets::text(
            ui,
            r.x + 120.0 * d,
            r.y + (r.h - 12.0 * 1.45 * d) / 2.0,
            12.0,
            Face::Regular,
            MUTED,
            &value,
        );
    }

    /// Buttons in a row from `x`; returns the right edge.
    fn buttons(&mut self, ui: &mut Ui, r: Rect, items: Vec<(String, bool, Action)>) {
        let d = ui.density;
        let mut x = r.x;
        for (label, on, action) in items {
            x = widgets::button_at(ui, &mut self.hits, x, r.y, r.h, &label, on, action) + 6.0 * d;
        }
    }

    fn footer(&mut self, ui: &mut Ui, r: Rect) {
        let d = ui.density;
        let row = Rect {
            x: r.x + 14.0 * d,
            y: r.y + r.h - 40.0 * d,
            w: r.w - 28.0 * d,
            h: 28.0 * d,
        };
        self.buttons(
            ui,
            row,
            vec![
                ("Duplicate".into(), false, Action::Duplicate),
                ("Delete".into(), false, Action::Delete),
                ("Close".into(), false, Action::Close),
            ],
        );
    }

    fn size_label(&self, w: Selected, screen: Screen) -> (String, String, f32, f32) {
        let r = self.rect_of(w, screen);
        let d = screen.density;
        let kind = match w {
            Selected::Button(l, i) => {
                self.controls.layout.layers[l].normal_buttons[i]
                    .button_size
                    .kind
            }
            Selected::Text(l, i) => {
                self.controls.layout.layers[l].text_boxes[i]
                    .button_size
                    .kind
            }
            Selected::Stick(l, i) => self.controls.layout.layers[l].joystick_buttons[i].size_type,
        };
        let unit = |px: f32, of: f32| match kind {
            SizeType::Dp => format!("{:.0} dp", px / d),
            SizeType::Percentage => format!("{:.1}%", px / of * 100.0),
            SizeType::WrapContent => "fits its text".to_owned(),
        };
        let (max_w, max_h, min) = (
            screen.width.min(400.0 * d),
            screen.height.min(400.0 * d),
            5.0 * d,
        );
        (
            unit(r.w, screen.height),
            unit(r.h, screen.height),
            (r.w - min) / (max_w - min),
            (r.h - min) / (max_h - min),
        )
    }

    fn dialog(&mut self, ui: &mut Ui, screen: Screen) {
        let Some(w) = self.selected else {
            return;
        };
        let d = screen.density;
        let tabs: &[(Tab, &str)] = match w {
            Selected::Button(..) => &[
                (Tab::Basic, "Basic"),
                (Tab::Text, "Text"),
                (Tab::Style, "Style"),
                (Tab::Events, "Events"),
            ],
            Selected::Text(..) => &[
                (Tab::Basic, "Basic"),
                (Tab::Text, "Text"),
                (Tab::Style, "Style"),
            ],
            Selected::Stick(..) => &[
                (Tab::Basic, "Basic"),
                (Tab::Joystick, "Joystick"),
                (Tab::Directions, "Directions"),
                (Tab::Style, "Style"),
            ],
        };
        if !tabs.iter().any(|(t, _)| *t == self.tab) {
            self.tab = Tab::Basic;
        }
        let r = self.frame(ui, screen, tabs);
        let rr = self.rect_of(w, screen);
        let all = catalogue(&self.controls.layout);
        match self.tab {
            Tab::Basic => {
                let (px, py) = match w {
                    Selected::Button(l, i) => {
                        let p = self.controls.layout.layers[l].normal_buttons[i].position;
                        (p.x, p.y)
                    }
                    Selected::Text(l, i) => {
                        let p = self.controls.layout.layers[l].text_boxes[i].position;
                        (p.x, p.y)
                    }
                    Selected::Stick(l, i) => {
                        let p = self.controls.layout.layers[l].joystick_buttons[i].position;
                        (p.x, p.y)
                    }
                };
                widgets::slider(
                    ui,
                    &mut self.hits,
                    Self::row(r, 0, d),
                    "Position X",
                    &format!("{:.1}%", px as f32 / 100.0),
                    px as f32 / 10000.0,
                    Action::Slide(Slide::X),
                );
                widgets::slider(
                    ui,
                    &mut self.hits,
                    Self::row(r, 1, d),
                    "Position Y",
                    &format!("{:.1}%", py as f32 / 100.0),
                    py as f32 / 10000.0,
                    Action::Slide(Slide::Y),
                );
                let (wl, hl, wf, hf) = self.size_label(w, screen);
                if let Selected::Stick(..) = w {
                    let f = (rr.w - 20.0 * d) / (screen.height - 20.0 * d);
                    widgets::slider(
                        ui,
                        &mut self.hits,
                        Self::row(r, 2, d),
                        "Size",
                        &wl,
                        f,
                        Action::Slide(Slide::Side),
                    );
                } else {
                    widgets::slider(
                        ui,
                        &mut self.hits,
                        Self::row(r, 2, d),
                        "Width",
                        &wl,
                        wf,
                        Action::Slide(Slide::W),
                    );
                    widgets::slider(
                        ui,
                        &mut self.hits,
                        Self::row(r, 3, d),
                        "Height",
                        &hl,
                        hf,
                        Action::Slide(Slide::H),
                    );
                }
                let visibility = match w {
                    Selected::Button(l, i) => {
                        self.controls.layout.layers[l].normal_buttons[i].visibility_type
                    }
                    Selected::Text(l, i) => {
                        self.controls.layout.layers[l].text_boxes[i].visibility_type
                    }
                    Selected::Stick(l, i) => {
                        self.controls.layout.layers[l].joystick_buttons[i].visibility_type
                    }
                };
                let kind_name = match self.size_label(w, screen).0.as_str() {
                    s if s.ends_with("dp") => "in dp",
                    s if s.ends_with('%') => "share of the screen",
                    _ => "fits its text",
                };
                self.buttons(
                    ui,
                    Self::row(r, 4, d),
                    vec![
                        (format!("Size: {kind_name}"), false, Action::SizeType),
                        (
                            format!("Shows: {}", visibility_name(visibility)),
                            false,
                            Action::Visibility,
                        ),
                    ],
                );
                Self::line(
                    ui,
                    Self::row(r, 5, d),
                    "Tip",
                    "drag the corner handle to resize",
                );
            }
            Tab::Text => {
                let (text, align, bold, italic, underline) = match w {
                    Selected::Button(l, i) => {
                        let b = &self.controls.layout.layers[l].normal_buttons[i];
                        (
                            b.text.default.clone(),
                            b.text_alignment,
                            b.text_bold,
                            b.text_italic,
                            b.text_underline,
                        )
                    }
                    Selected::Text(l, i) => {
                        let t = &self.controls.layout.layers[l].text_boxes[i];
                        (
                            t.text.default.clone(),
                            t.text_alignment,
                            t.text_bold,
                            t.text_italic,
                            t.text_underline,
                        )
                    }
                    Selected::Stick(..) => return,
                };
                let editing = self.typing == Some(Typing::Label);
                Self::line(
                    ui,
                    Self::row(r, 0, d),
                    "Text",
                    &if editing {
                        format!("{text}|")
                    } else {
                        text.replace('\n', " ")
                    },
                );
                self.buttons(
                    ui,
                    Self::row(r, 1, d),
                    vec![(
                        if editing {
                            "Done".into()
                        } else {
                            "Edit text".into()
                        },
                        editing,
                        Action::Type(Typing::Label),
                    )],
                );
                let align_name = match align {
                    TextAlignment::Left => "left",
                    TextAlignment::Center => "centre",
                    TextAlignment::Right => "right",
                };
                self.buttons(
                    ui,
                    Self::row(r, 2, d),
                    vec![(format!("Alignment: {align_name}"), false, Action::Align)],
                );
                self.buttons(
                    ui,
                    Self::row(r, 3, d),
                    vec![
                        ("Bold".into(), bold, Action::Bold),
                        ("Italic".into(), italic, Action::Italic),
                        ("Underline".into(), underline, Action::Underline),
                    ],
                );
            }
            Tab::Style => {
                let current = match w {
                    Selected::Button(l, i) => self.controls.layout.layers[l].normal_buttons[i]
                        .button_style
                        .clone(),
                    Selected::Text(l, i) => self.controls.layout.layers[l].text_boxes[i]
                        .button_style
                        .clone(),
                    Selected::Stick(l, i) => self.controls.layout.layers[l].joystick_buttons[i]
                        .joystick_style_id
                        .clone(),
                };
                let names: Vec<(Option<String>, String)> = if let Selected::Stick(..) = w {
                    std::iter::once((None, "Default".to_owned()))
                        .chain(
                            self.controls
                                .layout
                                .joystick_styles
                                .iter()
                                .map(|s| (Some(s.uuid.clone()), s.name.clone())),
                        )
                        .collect()
                } else {
                    std::iter::once((None, "Default".to_owned()))
                        .chain(
                            self.controls
                                .layout
                                .styles
                                .iter()
                                .map(|s| (Some(s.uuid.clone()), s.name.clone())),
                        )
                        .collect()
                };
                let stick = matches!(w, Selected::Stick(..));
                let mut n = 0;
                let mut row: Vec<(String, bool, Action)> = Vec::new();
                for (id, name) in names {
                    let on = current == id;
                    let action = if stick {
                        Action::StickStyle(id)
                    } else {
                        Action::Style(id)
                    };
                    row.push((name, on, action));
                    if row.len() == 2 {
                        self.buttons(ui, Self::row(r, n, d), std::mem::take(&mut row));
                        n += 1;
                    }
                    if n >= 6 {
                        break;
                    }
                }
                if !row.is_empty() && n < 6 {
                    self.buttons(ui, Self::row(r, n, d), row);
                }
            }
            Tab::Events => {
                let Some(b) = self.button_mut().map(|b| b.clone()) else {
                    return;
                };
                Self::line(
                    ui,
                    Self::row(r, 0, d),
                    "Sends",
                    &events_label(&b.click_events, &all),
                );
                self.buttons(
                    ui,
                    Self::row(r, 1, d),
                    vec![(
                        "Choose keys and actions".into(),
                        false,
                        Action::Pick(PickFor::Events),
                    )],
                );
                let yes = |v: bool| if v { "on" } else { "off" };
                self.buttons(
                    ui,
                    Self::row(r, 2, d),
                    vec![
                        (
                            format!("Look through: {}", yes(b.is_penetrable)),
                            b.is_penetrable,
                            Action::Penetrable,
                        ),
                        (
                            format!("Swipe: {}", yes(b.is_swipple)),
                            b.is_swipple,
                            Action::Swipe,
                        ),
                    ],
                );
                self.buttons(
                    ui,
                    Self::row(r, 3, d),
                    vec![(
                        format!("Toggle: {}", yes(b.is_toggleable)),
                        b.is_toggleable,
                        Action::Toggle,
                    )],
                );
                Self::line(
                    ui,
                    Self::row(r, 4, d),
                    "",
                    "Look through: the finger also turns the camera. Swipe: sliding onto it presses it.",
                );
                let send = b
                    .click_events
                    .iter()
                    .find(|e| e.kind == EventType::SendText)
                    .map(|e| e.key.clone())
                    .unwrap_or_default();
                let editing = self.typing == Some(Typing::SendText);
                Self::line(
                    ui,
                    Self::row(r, 5, d),
                    "Chat message",
                    &if editing {
                        format!("{send}|")
                    } else if send.is_empty() {
                        "none".into()
                    } else {
                        send
                    },
                );
                self.buttons(
                    ui,
                    Self::row(r, 6, d),
                    vec![(
                        if editing {
                            "Done".into()
                        } else {
                            "Edit message".into()
                        },
                        editing,
                        Action::Type(Typing::SendText),
                    )],
                );
            }
            Tab::Joystick => {
                let Some(j) = self.stick_mut().map(|j| j.clone()) else {
                    return;
                };
                widgets::slider(
                    ui,
                    &mut self.hits,
                    Self::row(r, 0, d),
                    "Dead zone",
                    &format!("{:.0}%", j.dead_zone_ratio * 100.0),
                    j.dead_zone_ratio / 0.9,
                    Action::Slide(Slide::DeadZone),
                );
                widgets::slider(
                    ui,
                    &mut self.hits,
                    Self::row(r, 1, d),
                    "Lock distance",
                    &format!("{:.0}%", j.lock_threshold * 100.0),
                    j.lock_threshold,
                    Action::Slide(Slide::LockThreshold),
                );
                self.buttons(
                    ui,
                    Self::row(r, 2, d),
                    vec![
                        (
                            format!("Forward lock: {}", if j.can_lock { "on" } else { "off" }),
                            j.can_lock,
                            Action::CanLock,
                        ),
                        (
                            format!(
                                "Starts: {}",
                                if j.trigger_mode == TriggerMode::Drag {
                                    "on drag"
                                } else {
                                    "on touch"
                                }
                            ),
                            false,
                            Action::Trigger,
                        ),
                    ],
                );
                Self::line(
                    ui,
                    Self::row(r, 3, d),
                    "Lock sends",
                    &events_label(&j.lock_events, &all),
                );
                self.buttons(
                    ui,
                    Self::row(r, 4, d),
                    vec![("Choose".into(), false, Action::Pick(PickFor::Lock))],
                );
            }
            Tab::Directions => {
                let Some(j) = self.stick_mut().map(|j| j.clone()) else {
                    return;
                };
                for (n, dir) in DIRECTIONS.iter().enumerate() {
                    let row = Self::row(r, n, d);
                    let events = j.direction_events.get(dir).cloned().unwrap_or_default();
                    let half = Rect {
                        w: row.w - 70.0 * d,
                        ..row
                    };
                    Self::line(ui, half, direction_name(*dir), &events_label(&events, &all));
                    widgets::button(
                        ui,
                        &mut self.hits,
                        Rect {
                            x: row.x + row.w - 64.0 * d,
                            w: 64.0 * d,
                            ..row
                        },
                        "Choose",
                        false,
                        Action::Pick(PickFor::Direction(*dir)),
                    );
                    if row.y + row.h > r.y + r.h - 44.0 * d {
                        break;
                    }
                }
            }
        }
        self.footer(ui, r);
    }

    fn layers(&mut self, ui: &mut Ui, screen: Screen) {
        let d = screen.density;
        let r = Rect {
            x: (screen.width - 520.0 * d) / 2.0,
            y: 56.0 * d,
            w: 520.0 * d,
            h: screen.height - 64.0 * d,
        };
        widgets::plate(ui, r, 12.0 * d, PLATE);
        widgets::text(
            ui,
            r.x + 14.0 * d,
            r.y + 14.0 * d,
            15.0,
            Face::Medium,
            TEXT,
            "Layers",
        );
        widgets::text(
            ui,
            r.x + 90.0 * d,
            r.y + 16.0 * d,
            12.0,
            Face::Regular,
            MUTED,
            "The first is on top. New widgets go to the selected one.",
        );
        let layers: Vec<(String, bool, Visibility, usize)> = self
            .controls
            .layout
            .layers
            .iter()
            .map(|l| {
                (
                    if l.name.is_empty() {
                        "layer".into()
                    } else {
                        l.name.clone()
                    },
                    l.hide,
                    l.visibility_type,
                    l.normal_buttons.len() + l.text_boxes.len() + l.joystick_buttons.len(),
                )
            })
            .collect();
        let max_rows = ((r.h - 100.0 * d) / (34.0 * d)) as usize;
        for (n, (name, hidden, shows, count)) in layers.into_iter().enumerate().take(max_rows) {
            let row = Self::row(r, n, d);
            let typing = self.typing == Some(Typing::LayerName(n));
            let title = if typing {
                format!("{name}|")
            } else {
                format!("{name} ({count})")
            };
            let selected = n == self.layer;
            let name_w = 150.0 * d;
            widgets::button(
                ui,
                &mut self.hits,
                Rect { w: name_w, ..row },
                &title,
                selected,
                Action::LayerSelect(n),
            );
            let mut x = row.x + name_w + 6.0 * d;
            for (label, on, action) in [
                (
                    if hidden { "Hidden" } else { "Shown" }.to_owned(),
                    !hidden,
                    Action::LayerHide(n),
                ),
                (
                    visibility_name(shows).to_owned(),
                    false,
                    Action::LayerShows(n),
                ),
                (
                    if typing { "Done" } else { "Rename" }.to_owned(),
                    typing,
                    Action::Type(Typing::LayerName(n)),
                ),
                ("Up".to_owned(), false, Action::LayerUp(n)),
                ("Delete".to_owned(), false, Action::LayerDelete(n)),
            ] {
                x = widgets::button_at(ui, &mut self.hits, x, row.y, row.h, &label, on, action)
                    + 4.0 * d;
            }
        }
        let row = Rect {
            x: r.x + 14.0 * d,
            y: r.y + r.h - 40.0 * d,
            w: r.w - 28.0 * d,
            h: 28.0 * d,
        };
        self.buttons(
            ui,
            row,
            vec![
                ("New layer".into(), true, Action::NewLayer),
                ("Close".into(), false, Action::Close),
            ],
        );
    }

    fn picker(&mut self, ui: &mut Ui, screen: Screen, target: PickFor) {
        let d = screen.density;
        let r = Rect {
            x: 24.0 * d,
            y: 56.0 * d,
            w: screen.width - 48.0 * d,
            h: screen.height - 64.0 * d,
        };
        widgets::plate(ui, r, 12.0 * d, PLATE);
        let chosen: Vec<ClickEvent> = match (target, self.selected) {
            (PickFor::Events, Some(Selected::Button(l, i))) => self.controls.layout.layers[l]
                .normal_buttons[i]
                .click_events
                .clone(),
            (PickFor::Lock, Some(Selected::Stick(l, i))) => self.controls.layout.layers[l]
                .joystick_buttons[i]
                .lock_events
                .clone(),
            (PickFor::Direction(dir), Some(Selected::Stick(l, i))) => {
                self.controls.layout.layers[l].joystick_buttons[i]
                    .direction_events
                    .get(&dir)
                    .cloned()
                    .unwrap_or_default()
            }
            _ => Vec::new(),
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
                widgets::button(
                    ui,
                    &mut self.hits,
                    Rect { x, y, w, h },
                    &label,
                    on,
                    Action::PickEvent(event),
                );
            }
            x += w + gap;
        }
        let mut bx = r.x + 14.0 * d;
        for (label, a, primary) in [
            ("Done", Action::PickDone, true),
            ("Up", Action::PickScroll(-(h + gap) * 3.0), false),
            ("Down", Action::PickScroll((h + gap) * 3.0), false),
        ] {
            bx = widgets::button_at(
                ui,
                &mut self.hits,
                bx,
                r.y + r.h - h - 12.0 * d,
                h,
                label,
                primary,
                a,
            ) + gap;
        }
    }
}
