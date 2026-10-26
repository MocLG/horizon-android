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
pub(super) fn text(ui: &mut Ui, x: f32, y: f32, size: f32, face: Face, colour: u32, s: &str) -> f32 {
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
    text(ui, r.x + (r.w - w) / 2.0, r.y + (r.h - line) / 2.0, size, face, colour, s);
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
    centred(ui, r, 13.0, Face::Medium, if primary { ON_ACCENT } else { TEXT }, label);
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
