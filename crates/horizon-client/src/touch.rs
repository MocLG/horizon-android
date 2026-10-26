//! Touch controls for a phone or tablet (Horizon's own; the Android app): a
//! stick that appears where the left thumb lands, a few buttons, and the rest
//! of the screen to look, tap and hold. This is their layout and drawing in
//! the design system; the game decides what a touch does.
use crate::hud::HudBatch;
use crate::ui_theme::{self, Face, Ui, color};

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Jump,
    Sneak,
    Inventory,
    Drop,
    Chat,
    Pause,
    Perspective,
}

impl Button {
    fn label(self) -> &'static str {
        match self {
            Button::Jump => "Jump",
            Button::Sneak => "Sneak",
            Button::Inventory => "Items",
            Button::Drop => "Drop",
            Button::Chat => "Chat",
            Button::Pause => "Pause",
            Button::Perspective => "View",
        }
    }
}

/// A round button: centre and radius in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl Disc {
    pub fn contains(&self, (x, y): (f32, f32)) -> bool {
        (x - self.x).hypot(y - self.y) <= self.r
    }
}

/// Where everything is on a screen, in physical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub buttons: Vec<(Button, Disc)>,
    /// The stick's radius; it appears where the thumb lands left of `stick_zone`.
    pub stick: f32,
    pub stick_zone: f32,
    /// The hotbar's nine slots: left edge, top and slot width.
    pub hotbar: (f32, f32, f32),
}

impl Layout {
    /// `dp` is physical pixels per density independent pixel; `size` scales
    /// the controls (1 is the default).
    pub fn new(width: f32, height: f32, dp: f32, size: f32, gui_scale: f32) -> Self {
        let u = dp * size;
        let big = 38.0 * u;
        let small = 24.0 * u;
        let edge = 22.0 * dp;
        let top = edge + small;
        // Vanilla's hotbar: 182 x 22 GUI pixels, centred at the bottom.
        let slot = 20.0 * gui_scale;
        let hotbar = ((width - 182.0 * gui_scale) / 2.0 + gui_scale, height - 22.0 * gui_scale, slot);
        let jump = Disc {
            x: width - edge - big - 14.0 * u,
            y: height - edge - big - 18.0 * u,
            r: big,
        };
        let sneak = Disc {
            x: jump.x - big - 34.0 * u,
            y: height - edge - small,
            r: small + 4.0 * u,
        };
        let right_of_hotbar = hotbar.0 + 9.0 * slot + small + 12.0 * u;
        let buttons = vec![
            (Button::Jump, jump),
            (Button::Sneak, sneak),
            (
                Button::Inventory,
                Disc {
                    x: right_of_hotbar.min(sneak.x - sneak.r - small - 8.0 * u),
                    y: height - small - 6.0 * u,
                    r: small,
                },
            ),
            (
                Button::Drop,
                Disc {
                    x: jump.x,
                    y: jump.y - big - small - 14.0 * u,
                    r: small,
                },
            ),
            (
                Button::Pause,
                Disc {
                    x: width - edge - small,
                    y: top,
                    r: small,
                },
            ),
            (
                Button::Chat,
                Disc {
                    x: width - edge - 3.0 * small - 10.0 * u,
                    y: top,
                    r: small,
                },
            ),
            (
                Button::Perspective,
                Disc {
                    x: width - edge - 5.0 * small - 20.0 * u,
                    y: top,
                    r: small,
                },
            ),
        ];
        Self {
            buttons,
            stick: 64.0 * u,
            stick_zone: width * 0.4,
            hotbar,
        }
    }

    pub fn button_at(&self, at: (f32, f32)) -> Option<Button> {
        self.buttons
            .iter()
            .find(|(_, d)| d.contains(at))
            .map(|(b, _)| *b)
    }

    /// The hotbar slot under `at`, if it is on the hotbar.
    pub fn slot_at(&self, (x, y): (f32, f32)) -> Option<usize> {
        let (left, top, slot) = self.hotbar;
        if y < top || x < left || x >= left + 9.0 * slot {
            return None;
        }
        Some(((x - left) / slot) as usize)
    }

    /// Whether a touch starting at `at` takes the stick.
    pub fn in_stick_zone(&self, (x, y): (f32, f32), height: f32) -> bool {
        x < self.stick_zone && y > height * 0.35
    }
}

/// A stick offset (in stick radii, -1..1 each way) as the movement keys:
/// forward, back, left, right, and sprinting when pushed fully forward.
pub fn stick_keys(dx: f32, dy: f32) -> [bool; 5] {
    let length = dx.hypot(dy);
    if length < 0.25 {
        return [false; 5];
    }
    let (x, y) = (dx / length, dy / length);
    // Eight ways: a diagonal holds two keys.
    let forward = y < -0.38;
    let back = y > 0.38;
    let left = x < -0.38;
    let right = x > 0.38;
    [forward, back, left, right, forward && length > 0.92 && y < -0.85]
}

/// What the controls show this frame.
#[derive(Default)]
pub struct State {
    /// The stick's centre and the thumb, in physical pixels.
    pub stick: Option<((f32, f32), (f32, f32))>,
    pub pressed: Vec<Button>,
    pub sneaking: bool,
    /// 0..1, the player's choice.
    pub opacity: f32,
}

/// Draws the controls over the HUD.
pub fn draw(batch: &mut HudBatch, atlas: &crate::hud::HudAtlas, layout: &Layout, state: &State) {
    let mut ui = Ui::new(atlas, batch, (0.0, 0.0));
    let d = ui.density;
    let o = state.opacity.clamp(0.1, 1.0);
    let plate = |a: f32| color::with_alpha(color::hex(horizon_theme::DARK.page, 255), a * o);
    let ink = |a: f32| color::with_alpha(color::TEXT, a * o);
    let disc = |ui: &mut Ui, c: Disc, fill: u32, ring: u32| {
        let (x, y, r) = (c.x / d, c.y / d, c.r / d);
        ui.rounded(x - r, y - r, 2.0 * r, 2.0 * r, r, fill);
        ui.border(x - r, y - r, 2.0 * r, 2.0 * r, r, ring);
    };
    for (button, at) in &layout.buttons {
        let on = state.pressed.contains(button) || (*button == Button::Sneak && state.sneaking);
        let fill = if on { ink(0.32) } else { plate(0.45) };
        disc(&mut ui, *at, fill, ink(0.35));
        let size = (at.r / d * 0.42).clamp(9.0, 15.0);
        let line = (size * 1.45).round();
        let label = button.label();
        let w = ui.measure(Face::Medium, size, label);
        ui.text_line(
            Face::Medium,
            size,
            line,
            at.x / d - w / 2.0,
            at.y / d - line / 2.0,
            ink(0.9),
            label,
        );
    }
    if let Some((centre, thumb)) = state.stick {
        let base = Disc {
            x: centre.0,
            y: centre.1,
            r: layout.stick,
        };
        disc(&mut ui, base, plate(0.3), ink(0.25));
        let knob = Disc {
            x: thumb.0,
            y: thumb.1,
            r: layout.stick * 0.42,
        };
        disc(&mut ui, knob, ink(0.35), ink(0.5));
    }
    let _ = ui_theme::RADIUS;
    ui.finish();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stick_moves_eight_ways_and_sprints_only_straight_ahead() {
        assert_eq!(stick_keys(0.1, 0.1), [false; 5]);
        assert_eq!(stick_keys(0.0, -0.6), [true, false, false, false, false]);
        assert_eq!(stick_keys(0.0, -1.0), [true, false, false, false, true]);
        assert_eq!(stick_keys(0.7, -0.7), [true, false, false, true, false]);
        assert_eq!(stick_keys(-0.8, 0.0), [false, false, true, false, false]);
        assert_eq!(stick_keys(0.0, 0.9), [false, true, false, false, false]);
    }

    #[test]
    fn controls_stay_on_screen_and_apart() {
        for (w, h, dp, gui) in [(2340.0, 1080.0, 2.625, 4.0), (1280.0, 720.0, 1.0, 2.0), (2560.0, 1600.0, 2.0, 4.0)] {
            let layout = Layout::new(w, h, dp, 1.0, gui);
            for (i, (_, a)) in layout.buttons.iter().enumerate() {
                assert!(a.x - a.r >= 0.0 && a.x + a.r <= w && a.y - a.r >= 0.0 && a.y + a.r <= h);
                for (_, b) in &layout.buttons[i + 1..] {
                    assert!((a.x - b.x).hypot(a.y - b.y) >= a.r + b.r, "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn hotbar_slots_follow_vanilla_geometry() {
        let layout = Layout::new(1280.0, 720.0, 1.0, 1.0, 2.0);
        let (left, top, slot) = layout.hotbar;
        assert_eq!(slot, 40.0);
        assert_eq!(layout.slot_at((left + 1.0, top + 2.0)), Some(0));
        assert_eq!(layout.slot_at((left + 8.5 * slot, top + 2.0)), Some(8));
        assert_eq!(layout.slot_at((left + 9.0 * slot + 1.0, top + 2.0)), None);
        assert_eq!(layout.slot_at((left + 1.0, top - 2.0)), None);
    }
}
