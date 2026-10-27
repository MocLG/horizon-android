use super::*;

const SCREEN: Screen = Screen {
    width: 2340.0,
    height: 1080.0,
    density: 2.625,
    dark: false,
};

fn centre_of(c: &Controls, label: &str) -> (f32, f32) {
    let (l, i) = c
        .layout
        .layers
        .iter()
        .enumerate()
        .find_map(|(l, layer)| {
            layer
                .normal_buttons
                .iter()
                .position(|b| b.text.default == label)
                .map(|i| (l, i))
        })
        .unwrap_or_else(|| panic!("no {label} button"));
    let r = c.button_rect((l, i), SCREEN);
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn keys(c: &mut Controls) -> Vec<(String, bool)> {
    c.take_events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Key { name, pressed } | Event::Launcher { name, pressed } => {
                Some((name, pressed))
            }
            Event::SendText(_) => None,
        })
        .collect()
}

#[test]
fn the_default_layout_reads_and_writes_back_the_same() {
    let layout = Layout::parse(DEFAULT_LAYOUT).unwrap();
    assert_eq!(layout.layers.len(), 5);
    assert_eq!(layout.styles.len(), 8);
    let again = Layout::parse(&layout.to_json()).unwrap();
    assert_eq!(layout, again);
}

#[test]
fn colours_read_both_long_forms() {
    // Kotlin wrote the old form as a signed long: negative when alpha is high.
    let old: Argb = serde_json::from_str(&((0x8000_0000u64 << 32) as i64).to_string()).unwrap();
    let older: Argb = serde_json::from_str(&(0x4000_0000u64 << 32).to_string()).unwrap();
    assert_eq!(older, Argb(0x4000_0000));
    let new: Argb = serde_json::from_str("2147483648").unwrap();
    assert_eq!(old, Argb(0x8000_0000));
    assert_eq!(new, Argb(0x8000_0000));
    assert_eq!(serde_json::to_string(&new).unwrap(), "2147483648");
}

#[test]
fn a_newer_layout_is_refused() {
    let mut json: serde_json::Value = serde_json::from_str(DEFAULT_LAYOUT).unwrap();
    json["editorVersion"] = 99.into();
    assert!(Layout::parse(&json.to_string()).is_err());
}

#[test]
fn a_button_sends_its_key_down_and_up() {
    let mut c = Controls::default_layout();
    let at = centre_of(&c, "E");
    assert_eq!(c.down(1, at, true, SCREEN), Claim::Taken);
    c.up(1, SCREEN);
    assert_eq!(
        keys(&mut c),
        [
            ("GLFW_KEY_E".to_owned(), true),
            ("GLFW_KEY_E".to_owned(), false)
        ]
    );
}

#[test]
fn penetrable_buttons_let_the_finger_look_too() {
    let mut c = Controls::default_layout();
    let at = centre_of(&c, "PRI");
    assert_eq!(c.down(1, at, true, SCREEN), Claim::MoveOnly);
    assert_eq!(keys(&mut c), [("GLFW_MOUSE_BUTTON_LEFT".to_owned(), true)]);
}

#[test]
fn a_toggle_stays_down_until_pressed_again() {
    let mut c = Controls::default_layout();
    let at = centre_of(&c, "□");
    c.down(1, at, true, SCREEN);
    c.up(1, SCREEN);
    assert_eq!(keys(&mut c), [("GLFW_KEY_LEFT_SHIFT".to_owned(), true)]);
    c.down(2, at, true, SCREEN);
    c.up(2, SCREEN);
    assert_eq!(keys(&mut c), [("GLFW_KEY_LEFT_SHIFT".to_owned(), false)]);
}

#[test]
fn the_gui_button_switches_the_other_layers() {
    let mut c = Controls::default_layout();
    let e = centre_of(&c, "E");
    let gui = centre_of(&c, "GUI");
    c.down(1, gui, true, SCREEN);
    c.up(1, SCREEN);
    assert_eq!(
        c.down(2, e, true, SCREEN),
        Claim::None,
        "the E button's layer is hidden"
    );
    c.down(3, gui, true, SCREEN);
    c.up(3, SCREEN);
    assert_eq!(c.down(4, e, true, SCREEN), Claim::Taken);
}

#[test]
fn in_game_layers_hide_in_menus() {
    let mut c = Controls::default_layout();
    let ctrl = centre_of(&c, "Ctrl");
    assert_eq!(c.down(1, ctrl, false, SCREEN), Claim::None);
    assert_eq!(c.down(2, ctrl, true, SCREEN), Claim::Taken);
}

#[test]
fn the_joystick_walks_eight_ways_and_locks_forward() {
    let mut c = Controls::default_layout();
    let (l, i) = (4, 0);
    let r = c.stick_rect((l, i), SCREEN);
    let centre = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    assert_eq!(c.down(1, centre, true, SCREEN), Claim::Taken);
    assert!(
        keys(&mut c).is_empty(),
        "a drag stick waits for the finger to move"
    );
    c.moved(1, (centre.0, r.y + 2.0), true, SCREEN);
    assert_eq!(keys(&mut c), [("GLFW_KEY_W".to_owned(), true)]);
    c.moved(1, (r.x + r.w - 2.0, centre.1 - r.h * 0.4), true, SCREEN);
    assert_eq!(
        keys(&mut c),
        [
            ("GLFW_KEY_W".to_owned(), false),
            ("GLFW_KEY_W".to_owned(), true),
            ("GLFW_KEY_D".to_owned(), true),
        ]
    );
    // Past the top edge: forward, and the lock (sprint) on letting go.
    c.moved(1, (centre.0, r.y - r.h), true, SCREEN);
    let events = keys(&mut c);
    assert!(
        events.contains(&("GLFW_KEY_LEFT_CONTROL".to_owned(), true)),
        "{events:?}"
    );
    c.up(1, SCREEN);
    assert!(c.sticks[l][i].locked);
    assert!(!keys(&mut c).contains(&("GLFW_KEY_W".to_owned(), false)));
}

#[test]
fn directions_follow_the_angle() {
    let c = (0.0, 0.0);
    assert_eq!(direction((10.0, 0.0), c, 1.0), Some(Direction::East));
    assert_eq!(direction((0.0, -10.0), c, 1.0), Some(Direction::North));
    assert_eq!(direction((-10.0, 0.0), c, 1.0), Some(Direction::West));
    assert_eq!(direction((-7.0, 7.0), c, 1.0), Some(Direction::SouthWest));
    assert_eq!(direction((0.5, 0.0), c, 1.0), None);
}

#[test]
fn the_editor_adds_and_moves_a_button() {
    use super::editor::{Editor, Outcome};
    let mut e = Editor::new(Controls::default_layout());
    let hud = |e: &mut Editor| {
        // Drawing lays out the toolbar and panel the taps land on.
        let atlas =
            crate::hud::HudAtlas::build(std::path::Path::new("/nonexistent/horizon-test-assets"))
                .unwrap();
        let mut batch = HudBatch::new([SCREEN.width as u32, SCREEN.height as u32]);
        e.draw(&mut batch, &atlas, SCREEN);
    };
    hud(&mut e);
    let before: usize = e.layout().layers[0].normal_buttons.len();
    let add = e.add_button_at().expect("the toolbar has Add button");
    e.down(9, add, SCREEN);
    e.up(9, add, SCREEN);
    let added = e.layout().layers[0].normal_buttons.len() > before;
    assert!(added, "the toolbar added no button");
    let (l, i) = (0, e.layout().layers[0].normal_buttons.len() - 1);
    // Drag it from the middle to the left edge.
    let r = e.controls.button_rect((l, i), SCREEN);
    let from = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    e.down(1, from, SCREEN);
    e.moved(1, (from.0 - 5000.0, from.1), SCREEN);
    assert_eq!(e.up(1, (from.0 - 5000.0, from.1), SCREEN), Outcome::None);
    assert_eq!(e.layout().layers[l].normal_buttons[i].position.x, 0);
    // The handle on its corner resizes it.
    hud(&mut e);
    let r = e.controls.button_rect((l, i), SCREEN);
    let corner = (r.x + r.w, r.y + r.h);
    e.down(2, corner, SCREEN);
    e.moved(2, (corner.0 + 100.0, corner.1 + 50.0), SCREEN);
    e.up(2, (corner.0 + 100.0, corner.1 + 50.0), SCREEN);
    let grown = e.controls.button_rect((l, i), SCREEN);
    assert!((grown.w - r.w - 100.0).abs() < 3.0, "{} to {}", r.w, grown.w);
    assert!((grown.h - r.h - 50.0).abs() < 3.0, "{} to {}", r.h, grown.h);
    let saved = e.layout().to_json();
    let layout = Layout::parse(&saved).unwrap();
    assert_eq!(layout.layers[l].normal_buttons.len(), before + 1);
}

#[test]
fn auto_size_keeps_the_default_and_shrinks_a_crowded_layout() {
    let mut c = Controls::default_layout();
    assert_eq!(c.fitting_size(SCREEN), 1.0);
    // Two 100 dp buttons a tenth of the free width apart, as a layout made
    // for a wider screen places them.
    let mut layout = c.layout.clone();
    let mut a = layout.layers[1].normal_buttons[0].clone();
    a.button_size.kind = SizeType::Dp;
    a.button_size.width_dp = 100.0;
    a.button_size.height_dp = 40.0;
    a.position = Position { x: 0, y: 5000 };
    let mut b = a.clone();
    b.position.x = 1000;
    layout.layers = vec![Layer {
        normal_buttons: vec![a, b],
        ..layout.layers[1].clone()
    }];
    let mut crowded = Controls::new(layout);
    let fit = crowded.fitting_size(SCREEN);
    assert!(fit < 1.0 && fit >= 0.5, "{fit}");
}

#[test]
fn older_layouts_are_updated_as_zalith_updates_them() {
    let mut json: serde_json::Value = serde_json::from_str(DEFAULT_LAYOUT).unwrap();
    json["editorVersion"] = 1.into();
    let x = json["layers"][1]["normalButtons"][1]["position"]["x"]
        .as_i64()
        .unwrap();
    json["layers"][1]["normalButtons"][1]["position"]["x"] = (x / 10).into();
    let layout = Layout::parse(&json.to_string()).unwrap();
    assert_eq!(
        i64::from(layout.layers[1].normal_buttons[1].position.x),
        x / 10 * 10
    );
    assert!(layout.styles.iter().all(|s| !s.common_style));
}
