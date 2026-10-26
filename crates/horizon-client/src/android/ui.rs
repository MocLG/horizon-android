//! The design system's widgets for the launcher (`docs/design-system.md`):
//! type does the work, lists instead of cards, one accent for the main action.
//! Immediate mode: a frame lays out and draws every widget, and a widget that
//! a finger tapped this frame says so.
use super::draw::{Canvas, Face, Rect, hex};
use horizon_theme::DARK as P;

pub const GUTTER: f32 = 24.0;
pub const ROW: f32 = 56.0;
pub const CONTROL: f32 = 36.0;

/// Where the finger is, and the tap the frame should act on.
#[derive(Default)]
pub struct Pointer {
    /// Down at, now at, and whether it moved far enough to be a drag.
    pub down: Option<((f32, f32), (f32, f32), bool)>,
    /// Released without dragging: a tap, for this frame.
    pub tap: Option<(f32, f32)>,
    /// Drag distance since the last frame, for scrolling.
    pub scroll: f32,
}

impl Pointer {
    pub fn press(&mut self, at: (f32, f32)) {
        self.down = Some((at, at, false));
    }
    pub fn moved(&mut self, at: (f32, f32)) {
        if let Some((start, now, dragged)) = &mut self.down {
            self.scroll += at.1 - now.1;
            *now = at;
            if (at.0 - start.0).hypot(at.1 - start.1) > 12.0 {
                *dragged = true;
            }
        }
    }
    pub fn release(&mut self) {
        if let Some((_, at, dragged)) = self.down.take()
            && !dragged
        {
            self.tap = Some(at);
        }
    }
    pub fn cancel(&mut self) {
        self.down = None;
    }
}

pub struct Ui<'c, 'a> {
    pub c: &'c mut Canvas<'a>,
    pointer: &'c Pointer,
    /// The tap, until a widget takes it.
    tap: Option<(f32, f32)>,
}

impl<'c, 'a> Ui<'c, 'a> {
    pub fn new(c: &'c mut Canvas<'a>, pointer: &'c Pointer) -> Self {
        let tap = pointer.tap;
        Self { c, pointer, tap }
    }

    /// Whether the tap of this frame landed in `r` (once).
    pub fn tapped(&mut self, r: Rect) -> bool {
        if self.tap.is_some_and(|t| r.contains(t)) {
            self.tap = None;
            true
        } else {
            false
        }
    }

    fn pressed(&self, r: Rect) -> bool {
        self.pointer
            .down
            .is_some_and(|(start, now, dragged)| !dragged && r.contains(start) && r.contains(now))
    }

    pub fn text(&mut self, x: f32, y: f32, text: &str, face: Face, size: f32, color: [f32; 4]) -> f32 {
        self.c.text(x, y, text, face, size, color)
    }

    /// Text centred in `r`.
    pub fn centred(&mut self, r: Rect, text: &str, face: Face, size: f32, color: [f32; 4]) {
        let w = self.c.measure(text, face, size);
        let h = self.c.line(face, size);
        self.c.text(
            r.x + (r.w - w) / 2.0,
            r.y + (r.h - h) / 2.0,
            text,
            face,
            size,
            color,
        );
    }

    /// A pill button: the accent for the screen's one main action, a soft
    /// plate otherwise. A disabled one draws faded and never fires.
    pub fn button(&mut self, r: Rect, label: &str, primary: bool, enabled: bool) -> bool {
        let pressed = enabled && self.pressed(r);
        let alpha = if !enabled {
            horizon_theme::DISABLED
        } else if pressed {
            horizon_theme::PRESSED
        } else {
            1.0
        };
        let (fill, text) = if primary {
            (hex(P.accent, alpha), hex(P.on_accent, 1.0))
        } else {
            (hex(P.plate, alpha), hex(P.ink, if enabled { 1.0 } else { 0.5 }))
        };
        self.c.rect(r, r.h / 2.0, fill);
        self.centred(r, label, Face::Medium, horizon_theme::UI.px, text);
        enabled && self.tapped(r)
    }

    /// The width a button needs for `label`.
    pub fn button_width(&mut self, label: &str) -> f32 {
        self.c.measure(label, Face::Medium, horizon_theme::UI.px) + 32.0
    }

    /// A button at the right end of a row, vertically centred on it.
    pub fn row_button(&mut self, row: Rect, label: &str, primary: bool, enabled: bool) -> bool {
        let w = self.button_width(label);
        let r = Rect::new(row.right() - w, row.y + (row.h - CONTROL) / 2.0, w, CONTROL);
        self.button(r, label, primary, enabled)
    }

    /// A switch at the right end of a row; returns the new value when tapped.
    pub fn switch(&mut self, row: Rect, on: bool) -> Option<bool> {
        let (w, h) = (40.0, 24.0);
        let r = Rect::new(row.right() - w, row.y + (row.h - h) / 2.0, w, h);
        self.c
            .rect(r, h / 2.0, if on { hex(P.accent, 1.0) } else { hex(P.plate, 1.0) });
        let knob = h - 6.0;
        let x = if on { r.right() - knob - 3.0 } else { r.x + 3.0 };
        self.c.rect(
            Rect::new(x, r.y + 3.0, knob, knob),
            knob / 2.0,
            if on { hex(P.on_accent, 1.0) } else { hex(P.muted, 1.0) },
        );
        self.tapped(Rect::new(row.x, row.y, row.w, row.h)).then_some(!on)
    }

    /// One row of a numbered list: the mono index, the name, a description
    /// under it, the rule below. Returns the area right of the text for the
    /// row's control.
    pub fn row(&mut self, r: Rect, index: usize, name: &str, detail: &str, control: f32) -> Rect {
        let index_text = format!("{index:02}");
        self.c.text(
            r.x,
            r.y + (r.h - self.c.line(Face::Mono, 12.0)) / 2.0,
            &index_text,
            Face::Mono,
            12.0,
            hex(P.muted, 1.0),
        );
        let x = r.x + 40.0;
        let width = (r.right() - control - 16.0 - x).max(40.0);
        let name_size = horizon_theme::UI.px + 1.0;
        let detail_size = horizon_theme::CAPTION.px;
        let lines: Vec<String> = if detail.is_empty() {
            Vec::new()
        } else {
            self.c
                .wrap(detail, Face::Regular, detail_size, width)
                .into_iter()
                .take(2)
                .collect()
        };
        let name_h = self.c.line(Face::Medium, name_size);
        let detail_h = self.c.line(Face::Regular, detail_size);
        let total = name_h + lines.len() as f32 * detail_h + if lines.is_empty() { 0.0 } else { 2.0 };
        let mut y = r.y + (r.h - total) / 2.0;
        let name = self.c.fit(name, Face::Medium, name_size, width);
        self.c.text(x, y, &name, Face::Medium, name_size, hex(P.ink, 1.0));
        y += name_h + 2.0;
        for line in lines {
            let line = self.c.fit(&line, Face::Regular, detail_size, width);
            self.c.text(x, y, &line, Face::Regular, detail_size, hex(P.muted, 1.0));
            y += detail_h;
        }
        self.c.rule(r.x, r.bottom(), r.w, hex(P.rule, 1.0));
        Rect::new(r.right() - control, r.y, control, r.h)
    }

    /// A thin progress track; `None` is a part that moves while the length is unknown.
    pub fn progress(&mut self, r: Rect, fraction: Option<f32>, time: f32) {
        self.c.rect(r, r.h / 2.0, hex(P.rule, 1.0));
        match fraction {
            Some(f) => {
                let w = r.w * f.clamp(0.0, 1.0);
                self.c
                    .rect(Rect::new(r.x, r.y, w, r.h), r.h / 2.0, hex(P.accent, 1.0));
            }
            None => {
                let part = r.w * 0.25;
                let t = (time * 0.6).fract();
                let x = r.x - part + (r.w + part) * t;
                let x0 = x.max(r.x);
                let x1 = (x + part).min(r.right());
                if x1 > x0 {
                    self.c
                        .rect(Rect::new(x0, r.y, x1 - x0, r.h), r.h / 2.0, hex(P.accent, 1.0));
                }
            }
        }
    }
}
