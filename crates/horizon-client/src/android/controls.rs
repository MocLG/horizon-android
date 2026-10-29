//! The player's control layouts, in Zalith Launcher 2's format
//! (`horizon_client::controls`): kept in the game folder's `controls/`, picked
//! by `controlLayout` in horizon.txt, and imported and exported through
//! Android's file picker, as is a backup of the touch settings.
use horizon_client::controls::layout::Layout;
use std::fs;
use std::path::Path;

/// Every layout to choose from, file stem and name: the default first.
pub fn layouts(home: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir(home.join("controls"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_owned();
            (path.extension()? == "json").then_some(())?;
            let layout = Layout::parse(&fs::read_to_string(&path).ok()?).ok()?;
            let name = match layout.info.name.default.trim() {
                "" => stem.clone(),
                name => name.to_owned(),
            };
            Some((stem, name))
        })
        .collect();
    found.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    let mut all = vec![(String::new(), "Default".to_owned())];
    all.extend(found);
    all
}

pub fn selected(home: &Path) -> String {
    horizon_client::ui_theme::preference(home, "controlLayout").unwrap_or_default()
}

/// The marker on the first line of a settings backup.
const SETTINGS_HEADER: &str = "# Horizon for Android: touch settings";

/// What a picked file imported as.
pub enum Imported {
    Layout(String),
    Settings(usize),
}

/// Imports a picked file: a settings backup, or a layout in Zalith Launcher
/// 2's format, kept in the game folder under the file's own name.
pub fn import(name: &str, text: &str, home: &Path) -> Result<Imported, String> {
    if text.starts_with(SETTINGS_HEADER) {
        return import_settings(text, home).map(Imported::Settings);
    }
    let layout = Layout::parse(text).map_err(|e| format!("{name}: {e}"))?;
    let stem: String = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layout")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let dir = home.join("controls");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("{stem}.json")), text).map_err(|e| e.to_string())?;
    let shown = layout.info.name.default.trim();
    Ok(Imported::Layout(if shown.is_empty() {
        stem
    } else {
        shown.to_owned()
    }))
}

/// The chosen layout, for Zalith Launcher 2 or another phone: its file name and text.
pub fn export(home: &Path) -> Result<(String, String), String> {
    let stem = selected(home);
    if stem.is_empty() {
        return Ok((
            "horizon-default.json".into(),
            horizon_client::controls::DEFAULT_LAYOUT.to_owned(),
        ));
    }
    let json = fs::read_to_string(home.join("controls").join(format!("{stem}.json")))
        .map_err(|e| e.to_string())?;
    Ok((format!("{stem}.json"), json))
}
/// The touch settings a backup carries: horizon.txt's, then options.txt's.
const SETTINGS: [&str; 8] = [
    "touchControls",
    "touchGestures",
    "touchGyroscope",
    "touchOpacity",
    "touchSize",
    "touchMenuBall",
    "largerInterface",
    "controlLayout",
];
const OPTIONS: [&str; 2] = ["guiScale", "debugGuiScale"];

fn read(file: &Path) -> Vec<(String, String)> {
    parse(&fs::read_to_string(file).unwrap_or_default())
}

fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

/// Sets `key` in a `key:value` file such as options.txt, keeping the rest.
fn write_option(file: &Path, key: &str, value: &str) -> Result<(), String> {
    let text = fs::read_to_string(file).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once(':').is_none_or(|(k, _)| k.trim() != key))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{key}:{value}"));
    fs::write(file, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// The touch settings as a backup: the controls' size and opacity, gestures,
/// the gyroscope, the menu ball, the interface, the GUI and debug screen
/// scales and the layout chosen.
pub fn export_settings(home: &Path) -> String {
    let ours = read(&home.join("horizon.txt"));
    let options = read(&home.join("options.txt"));
    let mut out = format!("{SETTINGS_HEADER}\n");
    for (keys, from) in [(&SETTINGS[..], &ours), (&OPTIONS[..], &options)] {
        for (k, v) in from.iter().filter(|(k, _)| keys.contains(&k.as_str())) {
            out.push_str(&format!("{k}:{v}\n"));
        }
    }
    out
}

/// Reads a backup `export_settings` wrote back into the game folder.
fn import_settings(text: &str, home: &Path) -> Result<usize, String> {
    let mut count = 0;
    for (k, v) in parse(text) {
        if SETTINGS.contains(&k.as_str()) {
            horizon_client::ui_theme::set_preference(home, &k, Some(&v));
        } else if OPTIONS.contains(&k.as_str()) {
            write_option(&home.join("options.txt"), &k, &v)?;
        } else {
            continue;
        }
        count += 1;
    }
    Ok(count)
}
