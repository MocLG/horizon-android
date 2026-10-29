//! The player's control layouts, in Zalith Launcher 2's format
//! (`horizon_client::controls`): kept in the game folder's `controls/`, picked
//! by `controlLayout` in horizon.txt, and copied in and out through a folder
//! a computer reaches over USB, as is a backup of the touch settings.
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

/// Copies the layouts in `from` that read as one into the game folder;
/// returns how many.
pub fn import(from: &Path, home: &Path) -> Result<usize, String> {
    let entries = match fs::read_dir(from) {
        Ok(entries) => entries,
        Err(_) => {
            fs::create_dir_all(from).map_err(|e| e.to_string())?;
            return Ok(0);
        }
    };
    let to = home.join("controls");
    fs::create_dir_all(&to).map_err(|e| e.to_string())?;
    let mut count = 0;
    let mut problems = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|json| Layout::parse(&json).map(|_| json))
        {
            Ok(json) => {
                fs::write(to.join(&name), json).map_err(|e| e.to_string())?;
                count += 1;
            }
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    if count == 0 && !problems.is_empty() {
        return Err(format!("No layout could be read. {}", problems.join(" ")));
    }
    Ok(count)
}

/// Writes every layout of the game folder to `to`, the default too when it is
/// the one chosen, for Zalith Launcher 2 or another phone; returns how many.
pub fn export(home: &Path, to: &Path) -> Result<usize, String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut count = 0;
    if selected(home).is_empty() {
        fs::write(
            to.join("horizon-default.json"),
            horizon_client::controls::DEFAULT_LAYOUT,
        )
        .map_err(|e| e.to_string())?;
        count += 1;
    }
    for (stem, _) in layouts(home)
        .into_iter()
        .filter(|(stem, _)| !stem.is_empty())
    {
        let name = format!("{stem}.json");
        fs::copy(home.join("controls").join(&name), to.join(&name)).map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
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
    fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
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

/// Writes the touch settings to `file`: the controls' size and opacity,
/// gestures, the gyroscope, the menu ball, the interface, the debug screen's
/// size and the layout chosen.
pub fn export_settings(home: &Path, file: &Path) -> Result<usize, String> {
    let ours = read(&home.join("horizon.txt"));
    let options = read(&home.join("options.txt"));
    let mut out = String::from("# Horizon for Android: touch settings\n");
    let mut count = 0;
    for (keys, from) in [(&SETTINGS[..], &ours), (&OPTIONS[..], &options)] {
        for (k, v) in from.iter().filter(|(k, _)| keys.contains(&k.as_str())) {
            out.push_str(&format!("{k}:{v}\n"));
            count += 1;
        }
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(file, out).map_err(|e| e.to_string())?;
    Ok(count)
}

/// Reads touch settings `export_settings` wrote back into the game folder.
pub fn import_settings(file: &Path, home: &Path) -> Result<usize, String> {
    if !file.exists() {
        return Err(format!("No settings to import at {}.", file.display()));
    }
    let mut count = 0;
    for (k, v) in read(file) {
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
