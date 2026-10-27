//! Updates from the app's GitHub releases (`android/release.sh`). Each
//! release is tagged `android-<versionCode>` and carries the APK, so a newer
//! one is a larger number than the installed app's. The APK downloads into
//! the cache folder, where `ApkProvider` hands it to Android's installer,
//! which asks the player. Android installs it only when it is signed with
//! the same key, so worlds, settings and accounts stay.
use super::install::Paths;
use super::jni;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/MocLG/horizon-android/releases/latest";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub code: u64,
    pub name: String,
    pub apk: String,
    pub size: u64,
}

/// The installed app's version code.
fn installed() -> Option<u64> {
    jni::platform("version_code", "")?.parse().ok()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .user_agent(&format!("horizon-android/{}", super::BUILD))
        .build()
}

/// The release a tag and its assets describe, if it has an APK.
fn read(json: &serde_json::Value) -> Option<Release> {
    let code = json["tag_name"].as_str()?.strip_prefix("android-")?.parse().ok()?;
    let asset = json["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".apk")))?;
    Some(Release {
        code,
        name: json["name"].as_str().unwrap_or_default().to_owned(),
        apk: asset["browser_download_url"].as_str()?.to_owned(),
        size: asset["size"].as_u64().unwrap_or(0),
    })
}

/// The latest release when it is newer than the installed app.
fn newer() -> Result<Option<Release>, String> {
    let json: serde_json::Value = agent()
        .get(LATEST)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let Some(installed) = installed() else {
        return Ok(None);
    };
    Ok(read(&json).filter(|r| r.code > installed))
}

/// Looks for an update on its own thread; the result lands in the returned slot.
pub fn check() -> Arc<Mutex<Option<Release>>> {
    let slot = Arc::new(Mutex::new(None));
    let found = slot.clone();
    std::thread::spawn(move || match newer() {
        Ok(Some(release)) => {
            log::info!("Horizon launcher: update {} ({}) is out", release.name, release.code);
            *found.lock().unwrap() = Some(release);
        }
        Ok(None) => {}
        Err(e) => log::info!("Horizon launcher: no update check: {e}"),
    });
    slot
}

/// Downloads the release's APK and opens Android's installer on it.
pub fn install(release: &Release, paths: &Paths, log: &dyn Fn(String)) -> Result<(), String> {
    std::fs::create_dir_all(&paths.cache).map_err(|e| e.to_string())?;
    let file = paths.cache.join("update.apk");
    let partial = paths.cache.join("update.apk.partial");
    let response = agent()
        .get(&release.apk)
        .call()
        .map_err(|e| format!("Could not download the update: {e}"))?;
    let mut reader = response.into_reader();
    let mut out = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut buf = vec![0; 1 << 16];
    let (mut done, mut shown) = (0u64, 0u64);
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("The update's download stopped: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if done - shown >= 1 << 20 {
            shown = done;
            log(format!("Downloading {} of {} MB", done >> 20, release.size >> 20));
        }
    }
    drop(out);
    if release.size != 0 && done != release.size {
        return Err(format!("The update downloaded {done} of {} bytes", release.size));
    }
    std::fs::rename(&partial, &file).map_err(|e| e.to_string())?;
    log("Opening the installer".to_owned());
    jni::platform("install_update", "");
    Ok(())
}

