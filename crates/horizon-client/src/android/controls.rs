//! The player's control layouts, in Zalith Launcher 2's format
//! (`horizon_client::controls`): kept in the game folder's `controls/`, picked
//! by `controlLayout` in horizon.txt, and copied in and out through a folder
//! a computer reaches over USB.
use horizon_client::controls::layout::Layout;
use std::fs;
use std::path::{Path, PathBuf};

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
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
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

/// Writes the current layout to `to` for Zalith Launcher 2 or another phone.
pub fn export(home: &Path, to: &Path) -> Result<PathBuf, String> {
    let stem = selected(home);
    let json = if stem.is_empty() {
        horizon_client::controls::DEFAULT_LAYOUT.to_owned()
    } else {
        fs::read_to_string(home.join("controls").join(format!("{stem}.json")))
            .map_err(|e| e.to_string())?
    };
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let file = to.join(format!(
        "{}.json",
        if stem.is_empty() { "horizon-default" } else { &stem }
    ));
    fs::write(&file, json).map_err(|e| e.to_string())?;
    Ok(file)
}
