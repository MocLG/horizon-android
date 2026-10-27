//! The control layout file, in Zalith Launcher 2's format (`editorVersion` 12),
//! so a layout exported there loads here and one saved here loads there:
//! layers of buttons, text boxes and joysticks, and the styles they share.
//! Positions are 0..10000 of the room the screen leaves around a widget;
//! percentage sizes are 0..10000 of the screen's width or height.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// The newest layout version this reads, the one it writes.
pub const EDITOR_VERSION: i32 = 12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub info: Info,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub styles: Vec<ButtonStyle>,
    #[serde(default)]
    pub joystick_styles: Vec<JoystickStyle>,
    pub editor_version: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub name: Text,
    pub author: Text,
    pub description: Text,
    #[serde(default)]
    pub version_code: i32,
    #[serde(default)]
    pub version_name: String,
}

/// A string with translations, picked by the system's language.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub default: String,
    #[serde(rename = "matchQueue", default)]
    pub match_queue: Vec<Localized>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Localized {
    pub language_tag: String,
    pub value: String,
}

impl Text {
    /// The text for a BCP 47 language tag (`"zh-CN"`): an exact match, else
    /// one for the language alone, else the default.
    pub fn translate(&self, language: &str) -> &str {
        let lower = language.to_ascii_lowercase();
        let base = lower.split(['-', '_']).next().unwrap_or("");
        self.match_queue
            .iter()
            .find(|l| {
                let tag = l.language_tag.to_ascii_lowercase().replace('_', "-");
                tag == lower.replace('_', "-") || tag == base
            })
            .map_or(&self.default, |l| &l.value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    #[default]
    #[serde(rename = "always")]
    Always,
    /// While the game has the pointer (in the world).
    #[serde(rename = "in_game")]
    InGame,
    /// While a screen has the pointer.
    #[serde(rename = "in_menu")]
    InMenu,
}

impl Visibility {
    pub fn shown(self, in_game: bool) -> bool {
        match self {
            Visibility::Always => true,
            Visibility::InGame => in_game,
            Visibility::InMenu => !in_game,
        }
    }
}

const fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    pub uuid: String,
    pub hide: bool,
    #[serde(default = "yes")]
    pub hide_when_mouse: bool,
    #[serde(default = "yes")]
    pub hide_when_gamepad: bool,
    pub visibility_type: Visibility,
    #[serde(default)]
    pub normal_buttons: Vec<Button>,
    #[serde(default)]
    pub text_boxes: Vec<TextBox>,
    #[serde(default)]
    pub joystick_buttons: Vec<Joystick>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeType {
    #[serde(rename = "dp")]
    Dp,
    #[serde(rename = "percentage")]
    Percentage,
    #[serde(rename = "wrap_content")]
    WrapContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reference {
    #[serde(rename = "screen_width")]
    ScreenWidth,
    #[serde(rename = "screen_height")]
    ScreenHeight,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    #[serde(rename = "type")]
    pub kind: SizeType,
    pub width_dp: f32,
    pub height_dp: f32,
    pub width_percentage: i32,
    pub height_percentage: i32,
    pub width_reference: Reference,
    pub height_reference: Reference,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// A key or a mouse button, by its GLFW name (`GLFW_KEY_E`).
    #[serde(rename = "key")]
    Key,
    /// One of the launcher's own (`launcher.event.switch_ime`), or a mouse
    /// button by its GLFW name.
    #[serde(rename = "launcher_event")]
    LauncherEvent,
    #[serde(rename = "switch_layer")]
    SwitchLayer,
    #[serde(rename = "show_layer")]
    ShowLayer,
    #[serde(rename = "hide_layer")]
    HideLayer,
    /// Chat text to send.
    #[serde(rename = "send_text")]
    SendText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClickEvent {
    #[serde(rename = "type")]
    pub kind: EventType,
    pub key: String,
}

impl ClickEvent {
    pub fn key(name: &str) -> Self {
        Self {
            kind: EventType::Key,
            key: name.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub text: Text,
    pub uuid: String,
    pub position: Position,
    pub button_size: Size,
    #[serde(default)]
    pub button_style: Option<String>,
    #[serde(default)]
    pub text_alignment: TextAlignment,
    #[serde(default)]
    pub text_bold: bool,
    #[serde(default)]
    pub text_italic: bool,
    #[serde(default)]
    pub text_underline: bool,
    pub visibility_type: Visibility,
    #[serde(default)]
    pub click_events: Vec<ClickEvent>,
    /// A finger sliding over it presses it, and leaving it releases it.
    pub is_swipple: bool,
    /// The finger also reaches what is under it (looking around).
    pub is_penetrable: bool,
    /// A press turns it on, the next one off.
    pub is_toggleable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBox {
    pub text: Text,
    pub uuid: String,
    pub position: Position,
    pub button_size: Size,
    #[serde(default)]
    pub button_style: Option<String>,
    #[serde(default)]
    pub text_alignment: TextAlignment,
    #[serde(default)]
    pub text_bold: bool,
    #[serde(default)]
    pub text_italic: bool,
    #[serde(default)]
    pub text_underline: bool,
    pub visibility_type: Visibility,
}

/// A colour as Zalith stores it: ARGB in the low 32 bits of a long (older
/// files put it in the high 32 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argb(pub u32);

impl Argb {
    pub const fn alpha(self) -> f32 {
        (self.0 >> 24) as f32 / 255.0
    }
}

impl Serialize for Argb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(i64::from(self.0))
    }
}

impl<'de> Deserialize<'de> for Argb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = match serde_json::Number::deserialize(d)? {
            n if n.is_i64() => n.as_i64().unwrap_or(0) as u64,
            n => n.as_u64().unwrap_or(0),
        };
        let high = (value >> 32) & 0xffff_ffff;
        Ok(Argb(if high != 0 { high as u32 } else { value as u32 }))
    }
}

/// Corner radii in dp: top start, top end, bottom end, bottom start.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub top_start: f32,
    pub top_end: f32,
    pub bottom_end: f32,
    pub bottom_start: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleConfig {
    pub alpha: f32,
    pub pressed_alpha: f32,
    pub background_color: Argb,
    pub pressed_background_color: Argb,
    pub content_color: Argb,
    pub pressed_content_color: Argb,
    /// sp; none is the text's own size.
    #[serde(default)]
    pub font_size: Option<i32>,
    #[serde(default)]
    pub pressed_font_size: Option<i32>,
    pub border_width: i32,
    pub pressed_border_width: i32,
    pub border_color: Argb,
    pub pressed_border_color: Argb,
    pub border_radius: Shape,
    pub pressed_border_radius: Shape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonStyle {
    pub name: String,
    pub uuid: String,
    pub animate_swap: bool,
    /// The light style in a dark system theme too.
    #[serde(default = "yes")]
    pub common_style: bool,
    pub light_style: StyleConfig,
    pub dark_style: StyleConfig,
}

impl ButtonStyle {
    pub fn config(&self, dark: bool) -> &StyleConfig {
        if self.common_style || !dark {
            &self.light_style
        } else {
            &self.dark_style
        }
    }
}

/// The style a button without one, or with a missing one, draws in.
pub fn default_style() -> StyleConfig {
    let square = Shape {
        top_start: 0.0,
        top_end: 0.0,
        bottom_end: 0.0,
        bottom_start: 0.0,
    };
    StyleConfig {
        alpha: 1.0,
        pressed_alpha: 1.0,
        background_color: Argb(0x8000_0000),
        pressed_background_color: Argb(0xb388_8888),
        content_color: Argb(0xffff_ffff),
        pressed_content_color: Argb(0xffff_ffff),
        font_size: None,
        pressed_font_size: None,
        border_width: 0,
        pressed_border_width: 0,
        border_color: Argb(0xffff_ffff),
        pressed_border_color: Argb(0xffff_ffff),
        border_radius: square,
        pressed_border_radius: square,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Direction {
    #[serde(rename = "east")]
    East,
    #[serde(rename = "north_east")]
    NorthEast,
    #[serde(rename = "north")]
    North,
    #[serde(rename = "north_west")]
    NorthWest,
    #[serde(rename = "west")]
    West,
    #[serde(rename = "south_west")]
    SouthWest,
    #[serde(rename = "south")]
    South,
    #[serde(rename = "south_east")]
    SouthEast,
    #[serde(rename = "none")]
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    /// The knob moves once the finger drags.
    #[default]
    #[serde(rename = "drag")]
    Drag,
    /// The knob jumps under the finger as it lands.
    #[serde(rename = "touch")]
    Touch,
}

fn percentage() -> SizeType {
    SizeType::Percentage
}
fn size_dp() -> f32 {
    200.0
}
fn size_percentage() -> i32 {
    2500
}
fn dead_zone() -> f32 {
    0.5
}
fn lock_threshold() -> f32 {
    0.3
}

/// WASD, two keys on the diagonals.
pub fn default_direction_events() -> BTreeMap<Direction, Vec<ClickEvent>> {
    let (w, a, s, d) = (
        ClickEvent::key("GLFW_KEY_W"),
        ClickEvent::key("GLFW_KEY_A"),
        ClickEvent::key("GLFW_KEY_S"),
        ClickEvent::key("GLFW_KEY_D"),
    );
    BTreeMap::from([
        (Direction::North, vec![w.clone()]),
        (Direction::NorthEast, vec![w.clone(), d.clone()]),
        (Direction::NorthWest, vec![w, a.clone()]),
        (Direction::South, vec![s.clone()]),
        (Direction::SouthEast, vec![s.clone(), d.clone()]),
        (Direction::SouthWest, vec![s, a.clone()]),
        (Direction::East, vec![d]),
        (Direction::West, vec![a]),
    ])
}

fn default_lock_events() -> Vec<ClickEvent> {
    vec![ClickEvent::key("GLFW_KEY_LEFT_CONTROL")]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Joystick {
    pub uuid: String,
    pub position: Position,
    #[serde(default = "percentage")]
    pub size_type: SizeType,
    #[serde(default = "size_dp")]
    pub size_dp: f32,
    #[serde(default = "size_percentage")]
    pub size_percentage: i32,
    #[serde(default)]
    pub visibility_type: Visibility,
    #[serde(default)]
    pub joystick_style_id: Option<String>,
    #[serde(default = "dead_zone")]
    pub dead_zone_ratio: f32,
    /// How far past the top edge (of the radius) the finger goes to lock forward.
    #[serde(default = "lock_threshold")]
    pub lock_threshold: f32,
    #[serde(default = "yes")]
    pub can_lock: bool,
    #[serde(default)]
    pub trigger_mode: TriggerMode,
    #[serde(default = "default_direction_events")]
    pub direction_events: BTreeMap<Direction, Vec<ClickEvent>>,
    #[serde(default = "default_lock_events")]
    pub lock_events: Vec<ClickEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoystickStyleConfig {
    pub alpha: f32,
    pub background_color: Argb,
    pub joystick_color: Argb,
    pub joystick_can_lock_color: Argb,
    pub joystick_locked_color: Argb,
    pub lock_mark_color: Argb,
    #[serde(default)]
    pub border_width_ratio: i32,
    pub border_color: Argb,
    /// Corner radius as a percentage of the size, 0..50.
    pub background_shape: i32,
    pub joystick_shape: i32,
    /// The knob's size as a share of the background's.
    pub joystick_size: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoystickStyle {
    pub name: String,
    pub uuid: String,
    #[serde(default = "yes")]
    pub common_style: bool,
    pub light_style: JoystickStyleConfig,
    pub dark_style: JoystickStyleConfig,
}

impl JoystickStyle {
    pub fn config(&self, dark: bool) -> &JoystickStyleConfig {
        if self.common_style || !dark {
            &self.light_style
        } else {
            &self.dark_style
        }
    }
}

pub fn default_joystick_style() -> JoystickStyleConfig {
    JoystickStyleConfig {
        alpha: 1.0,
        background_color: Argb(0x8000_0000),
        joystick_color: Argb(0x80ff_ffff),
        joystick_can_lock_color: Argb(0x80ff_ff00),
        joystick_locked_color: Argb(0x8000_ff00),
        lock_mark_color: Argb(0xffff_ffff),
        border_width_ratio: 0,
        border_color: Argb(0xffff_ffff),
        background_shape: 50,
        joystick_shape: 50,
        joystick_size: 0.5,
    }
}

/// Older layouts kept their one joystick style beside the layers.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Legacy {
    special: Option<LegacySpecial>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySpecial {
    joystick_style: Option<JoystickStyleConfig>,
}

impl Layout {
    /// Reads a layout file; a newer version than this one reads is refused,
    /// as Zalith refuses one.
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let version = value
            .get("editorVersion")
            .and_then(serde_json::Value::as_i64)
            .ok_or("The file has no \"editorVersion\": it is not a control layout")?;
        if version > i64::from(EDITOR_VERSION) {
            return Err(format!(
                "This control layout is version {version}; Horizon reads up to {EDITOR_VERSION}"
            ));
        }
        let mut layout: Layout =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        if version < 12
            && let Ok(Legacy {
                special:
                    Some(LegacySpecial {
                        joystick_style: Some(style),
                    }),
            }) = serde_json::from_value::<Legacy>(value)
        {
            layout.joystick_styles.push(JoystickStyle {
                name: "Default".into(),
                uuid: "legacy".into(),
                common_style: true,
                light_style: style.clone(),
                dark_style: style,
            });
        }
        // Zalith's own updates of older layouts (Version.kt).
        if version < 2 {
            // Positions and sizes went from 0..1000 to 0..10000.
            for layer in &mut layout.layers {
                for b in &mut layer.normal_buttons {
                    b.position.x *= 10;
                    b.position.y *= 10;
                    b.button_size.width_percentage *= 10;
                    b.button_size.height_percentage *= 10;
                }
                for t in &mut layer.text_boxes {
                    t.position.x *= 10;
                    t.position.y *= 10;
                    t.button_size.width_percentage *= 10;
                    t.button_size.height_percentage *= 10;
                }
            }
        }
        if version < 11 {
            // Older styles had light and dark variants, picked by the system's theme.
            for style in &mut layout.styles {
                style.common_style = false;
            }
        }
        layout.editor_version = EDITOR_VERSION;
        Ok(layout)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn style(&self, id: Option<&str>) -> Option<&ButtonStyle> {
        let id = id?;
        self.styles.iter().find(|s| s.uuid == id)
    }

    pub fn joystick_style(&self, id: Option<&str>) -> Option<&JoystickStyle> {
        let id = id?;
        self.joystick_styles.iter().find(|s| s.uuid == id)
    }
}
