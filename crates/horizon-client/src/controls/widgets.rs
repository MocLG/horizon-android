//! The plates, buttons and text the layout editor and the game menu draw, in
//! the design system's dark palette, laid out in physical pixels. A frame
//! records where each action's button is; a tap looks it up.
use super::Rect;
use crate::ui_theme::{Face, Ui, color};

pub(super) struct Hits<A> {
    list: Vec<(A, Rect)>,
}

impl<A: Clone> Hits<A> {
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }
    pub fn clear(&mut self) {
        self.list.clear();
    }
    pub fn push(&mut self, action: A, r: Rect) {
        self.list.push((action, r));
    }
    /// The centre of `action`'s button.
    #[cfg(test)]
    pub fn find(&self, action: &A) -> Option<(f32, f32)>
    where
        A: PartialEq,
    {
        self.list
            .iter()
            .find(|(a, _)| a == action)
            .map(|(_, r)| (r.x + r.w / 2.0, r.y + r.h / 2.0))
    }
    /// The action at `at` and its area, the last drawn (topmost) first.
    pub fn hit(&self, at: (f32, f32)) -> Option<(A, Rect)> {
        self.list
            .iter()
            .rev()
            .find(|(_, r)| r.contains(at))
            .cloned()
    }
    /// The action at `at`, the last drawn (topmost) first.
    pub fn at(&self, at: (f32, f32)) -> Option<A> {
        self.list
            .iter()
            .rev()
            .find(|(_, r)| r.contains(at))
            .map(|(a, _)| a.clone())
    }
}

pub(super) const PLATE: u32 = color::hex(horizon_theme::DARK.surface, 240);
pub(super) const RAISED: u32 = color::hex(horizon_theme::DARK.plate, 255);
pub(super) const TEXT: u32 = color::TEXT;
pub(super) const MUTED: u32 = color::MUTED;
pub(super) const ACCENT: u32 = color::ACCENT;
pub(super) const ON_ACCENT: u32 = color::ON_ACCENT;

pub(super) fn plate(ui: &mut Ui, r: Rect, radius: f32, colour: u32) {
    ui.rounded_px(
        r.x.round(),
        r.y.round(),
        (r.x + r.w).round(),
        (r.y + r.h).round(),
        [radius; 4],
        colour,
    );
}

/// Text of `size` dp with its box's top left at `(x, y)` physical pixels; returns its width in pixels.
pub(super) fn text(
    ui: &mut Ui,
    x: f32,
    y: f32,
    size: f32,
    face: Face,
    colour: u32,
    s: &str,
) -> f32 {
    let d = ui.density;
    let line = (size * 1.45).round();
    ui.text_line(face, size, line, x / d, y / d, colour, s) * d
}

pub(super) fn text_width(ui: &Ui, size: f32, face: Face, s: &str) -> f32 {
    ui.measure(face, size, s) * ui.density
}

/// Text centred in `r`.
pub(super) fn centred(ui: &mut Ui, r: Rect, size: f32, face: Face, colour: u32, s: &str) {
    let d = ui.density;
    let w = text_width(ui, size, face, s);
    let line = (size * 1.45).round() * d;
    text(
        ui,
        r.x + (r.w - w) / 2.0,
        r.y + (r.h - line) / 2.0,
        size,
        face,
        colour,
        s,
    );
}

/// A pill button; `primary` in the accent.
pub(super) fn button<A: Clone>(
    ui: &mut Ui,
    hits: &mut Hits<A>,
    r: Rect,
    label: &str,
    primary: bool,
    action: A,
) {
    plate(ui, r, r.h / 2.0, if primary { ACCENT } else { RAISED });
    centred(
        ui,
        r,
        13.0,
        Face::Medium,
        if primary { ON_ACCENT } else { TEXT },
        label,
    );
    hits.push(action, r);
}

/// A button as wide as its label needs, at `(x, y)`; returns its right edge.
pub(super) fn button_at<A: Clone>(
    ui: &mut Ui,
    hits: &mut Hits<A>,
    x: f32,
    y: f32,
    h: f32,
    label: &str,
    primary: bool,
    action: A,
) -> f32 {
    let w = text_width(ui, 13.0, Face::Medium, label) + h;
    button(ui, hits, Rect { x, y, w, h }, label, primary, action);
    x + w
}

/// A slider: its name and value on the left, a track a finger drags; the
/// action marks where it is so a press starts dragging it.
#[allow(clippy::too_many_arguments)]
pub(super) fn slider<A: Clone>(
    ui: &mut Ui,
    hits: &mut Hits<A>,
    r: Rect,
    name: &str,
    value: &str,
    fraction: f32,
    action: A,
) {
    let d = ui.density;
    let label_w = 120.0 * d;
    text(
        ui,
        r.x,
        r.y + (r.h - 13.0 * 1.45 * d) / 2.0,
        13.0,
        Face::Medium,
        TEXT,
        name,
    );
    let vw = text_width(ui, 12.0, Face::Regular, value);
    text(
        ui,
        r.x + label_w - vw - 8.0 * d,
        r.y + (r.h - 12.0 * 1.45 * d) / 2.0,
        12.0,
        Face::Regular,
        MUTED,
        value,
    );
    let track = Rect {
        x: r.x + label_w,
        y: r.y + r.h / 2.0 - 2.0 * d,
        w: r.w - label_w,
        h: 4.0 * d,
    };
    plate(ui, track, track.h / 2.0, RAISED);
    let f = fraction.clamp(0.0, 1.0);
    plate(
        ui,
        Rect {
            w: track.w * f,
            ..track
        },
        track.h / 2.0,
        ACCENT,
    );
    let knob = 16.0 * d;
    plate(
        ui,
        Rect {
            x: track.x + track.w * f - knob / 2.0,
            y: r.y + (r.h - knob) / 2.0,
            w: knob,
            h: knob,
        },
        knob / 2.0,
        TEXT,
    );
    hits.push(
        action,
        Rect {
            x: track.x - knob,
            y: r.y,
            w: track.w + 2.0 * knob,
            h: r.h,
        },
    );
}

/// Where along a slider whose hit area is `hit` the finger at `x` is, 0..1.
pub(super) fn slider_fraction(hit: Rect, x: f32, density: f32) -> f32 {
    let knob = 16.0 * density;
    ((x - hit.x - knob) / (hit.w - 2.0 * knob)).clamp(0.0, 1.0)
}
