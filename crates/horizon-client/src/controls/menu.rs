//! The game menu, after Zalith Launcher 2's floating ball: a ball over the
//! game that drags anywhere and, tapped, opens the menu (the layout editor,
//! gestures, the gyroscope, the controls' opacity, the keyboard).
use super::widgets::{self, Hits, MUTED, PLATE, TEXT};
use super::*;
use crate::ui_theme::Face;

/// The player's touch settings the menu changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// A tap on the world uses, a hold attacks (Zalith's gesture control).
    pub gestures: bool,
    pub gyroscope: bool,
    /// The controls' opacity, 0..1.
    pub opacity: f32,
    pub controls_hidden: bool,
    /// The controls' size; none fits the layout to the screen.
    pub size: Option<f32>,
    /// The size Auto picked, to show.
    pub fitted: f32,
    /// GUI scales past vanilla's largest (Horizon's, off by default).
    pub larger_interface: bool,
    /// Vanilla's `debugGuiScale`: -1 the GUI's, 0 half the largest, else a scale.
    pub debug_scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    EditControls,
    Gestures,
    Gyroscope,
    Opacity,
    HideControls,
    Size,
    Interface,
    DebugSize,
    Keyboard,
    Pause,
    Close,
}

pub struct Menu {
    /// The ball's centre as a share of the screen.
    pub ball: (f32, f32),
    pub open: bool,
    hits: Hits<Choice>,
    /// A finger on the ball: where it landed, and whether it dragged.
    on_ball: Option<(u64, (f32, f32), bool)>,
    pending: Option<(u64, Choice)>,
}

impl Menu {
    pub fn new(ball: (f32, f32)) -> Self {
        Self {
            ball,
            open: false,
            hits: Hits::new(),
            on_ball: None,
            pending: None,
        }
    }

    fn ball_disc(&self, screen: Screen) -> (f32, f32, f32) {
        (
            self.ball.0 * screen.width,
            self.ball.1 * screen.height,
            20.0 * screen.density,
        )
    }

    /// Whether the menu takes a finger landing at `at`.
    pub fn down(&mut self, finger: u64, at: (f32, f32), screen: Screen) -> bool {
        let (x, y, r) = self.ball_disc(screen);
        if (at.0 - x).hypot(at.1 - y) <= r * 1.3 {
            self.on_ball = Some((finger, at, false));
            return true;
        }
        if !self.open {
            return false;
        }
        if let Some(choice) = self.hits.at(at) {
            self.pending = Some((finger, choice));
        }
        true
    }

    pub fn moved(&mut self, finger: u64, at: (f32, f32), screen: Screen) {
        if let Some((f, start, dragged)) = &mut self.on_ball
            && *f == finger
        {
            if !*dragged && (at.0 - start.0).hypot(at.1 - start.1) > 8.0 * screen.density {
                *dragged = true;
            }
            if *dragged {
                self.ball = (
                    (at.0 / screen.width).clamp(0.02, 0.98),
                    (at.1 / screen.height).clamp(0.03, 0.97),
                );
            }
        }
    }

    /// The choice a lifted finger made, if any. A tap on the ball opens or closes the menu.
    pub fn up(&mut self, finger: u64, at: (f32, f32)) -> Option<Choice> {
        if let Some((f, _, dragged)) = self.on_ball
            && f == finger
        {
            self.on_ball = None;
            if !dragged {
                self.open = !self.open;
            }
            return None;
        }
        let (f, choice) = self.pending.take()?;
        (f == finger && self.hits.at(at) == Some(choice)).then_some(choice)
    }

    pub fn holds(&self, finger: u64) -> bool {
        self.on_ball.is_some_and(|(f, _, _)| f == finger) || self.pending.is_some_and(|(f, _)| f == finger)
    }

    pub fn draw(&mut self, batch: &mut HudBatch, atlas: &HudAtlas, screen: Screen, settings: Settings) {
        let mut ui = Ui::new(atlas, batch, (0.0, 0.0));
        self.hits.clear();
        let d = screen.density;
        let (x, y, r) = self.ball_disc(screen);
        ui.rounded_px((x - r).round(), (y - r).round(), (x + r).round(), (y + r).round(), [r; 4], PLATE);
        ui.border_px(
            (x - r).round(),
            (y - r).round(),
            (x + r).round(),
            (y + r).round(),
            [r; 4],
            1.0f32.max(d.round()),
            widgets::ACCENT,
        );
        widgets::centred(
            &mut ui,
            Rect { x: x - r, y: y - r, w: 2.0 * r, h: 2.0 * r },
            13.0,
            Face::SemiBold,
            TEXT,
            "H",
        );
        if self.open {
            let w = 560.0 * d;
            let row = 40.0 * d;
            let on = |v: bool| if v { "on" } else { "off" };
            let items = [
                ("Edit controls".to_owned(), Choice::EditControls),
                (format!("Gestures: {}", on(settings.gestures)), Choice::Gestures),
                (format!("Gyroscope: {}", on(settings.gyroscope)), Choice::Gyroscope),
                (format!("Controls opacity: {:.0}%", settings.opacity * 100.0), Choice::Opacity),
                (
                    match settings.size {
                        None => format!("Controls size: Auto ({:.0}%)", settings.fitted * 100.0),
                        Some(s) => format!("Controls size: {:.0}%", s * 100.0),
                    },
                    Choice::Size,
                ),
                (
                    format!(
                        "Interface: {}",
                        if settings.larger_interface { "larger than vanilla" } else { "vanilla" }
                    ),
                    Choice::Interface,
                ),
                (
                    format!(
                        "Debug screen (F3): {}",
                        match settings.debug_scale {
                            -1 => "as the GUI".to_owned(),
                            0 => "vanilla's".to_owned(),
                            n => format!("scale {n}"),
                        }
                    ),
                    Choice::DebugSize,
                ),
                (
                    if settings.controls_hidden { "Show controls" } else { "Hide controls" }.to_owned(),
                    Choice::HideControls,
                ),
                ("Keyboard".to_owned(), Choice::Keyboard),
                ("Pause".to_owned(), Choice::Pause),
                ("Close".to_owned(), Choice::Close),
            ];
            let rows = items.len().div_ceil(2);
            let h = row * rows as f32 + 56.0 * d;
            let p = Rect {
                x: (screen.width - w) / 2.0,
                y: ((screen.height - h) / 2.0).max(8.0 * d),
                w,
                h,
            };
            widgets::plate(&mut ui, p, 14.0 * d, PLATE);
            widgets::text(&mut ui, p.x + 18.0 * d, p.y + 14.0 * d, 15.0, Face::Medium, TEXT, "Horizon");
            widgets::text(
                &mut ui,
                p.x + 90.0 * d,
                p.y + 16.0 * d,
                12.0,
                Face::Regular,
                MUTED,
                "Drag the ball anywhere",
            );
            let column = (p.w - 34.0 * d) / 2.0;
            for (n, (label, choice)) in items.into_iter().enumerate() {
                let b = Rect {
                    x: p.x + 14.0 * d + (n % 2) as f32 * (column + 6.0 * d),
                    y: p.y + 48.0 * d + (n / 2) as f32 * row,
                    w: column,
                    h: row - 6.0 * d,
                };
                widgets::button(&mut ui, &mut self.hits, b, &label, choice == Choice::EditControls, choice);
            }
        }
        ui.finish();
    }
}
