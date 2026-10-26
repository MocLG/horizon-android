//! What the desktop launcher does before Play, on the phone: the player's own
//! Minecraft from Mojang (or a Minecraft folder they copied over), the server
//! jar, then horizon-prepare with the Java runtime the app carries. Nothing
//! of Mojang's is in the app; it all comes from the player's account.
use crate::{account, mojang, sources};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use winit::platform::android::activity::AndroidApp;

/// The app's folders.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The launcher's data: accounts, the downloaded game, the server jar.
    pub data: PathBuf,
    /// Horizon's version folder (research-lock.json, the exporters), where
    /// the prepared resources land under `.local`.
    pub root: PathBuf,
    /// The Java runtime, unpacked from the app.
    pub jre: PathBuf,
    /// The game's folder: worlds, options, logs. In the app's internal
    /// storage: the external one may not hold the worlds' session locks.
    pub home: PathBuf,
    /// Where a player can copy a Minecraft folder over USB: the app's
    /// external storage.
    pub shared: PathBuf,
    /// The app's native libraries, where the java launcher is.
    pub libs: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    pub fn new(app: &AndroidApp) -> Self {
        let internal = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp/horizon"));
        let shared = app
            .external_data_path()
            .filter(|p| fs::create_dir_all(p).is_ok())
            .unwrap_or_else(|| internal.join("shared"));
        let libs = super::jni::platform("native_lib_dir", "")
            .map(PathBuf::from)
            .unwrap_or_default();
        Self {
            data: internal.join("launcher"),
            root: internal.join("horizon"),
            jre: internal.join("runtimes").join("jre-25"),
            home: internal.join("game"),
            shared,
            libs,
            cache: internal.join("cache"),
        }
    }
    pub fn resources(&self) -> PathBuf {
        self.root
            .join(".local")
            .join("horizon-client")
            .join(horizon_core::VANILLA_VERSION)
    }
    /// A finished preparation writes this last.
    pub fn prepared(&self) -> bool {
        self.resources().join("horizon-resources.json").is_file()
            && self.root.join(".local").join("prepared-by-app").is_file()
    }
    pub fn java(&self) -> PathBuf {
        self.libs.join("libhorizonjava.so")
    }
    /// The Minecraft folder a player copied into the game's folder, if any.
    pub fn copied_minecraft(&self) -> PathBuf {
        self.shared.join("minecraft")
    }
    pub fn downloaded_minecraft(&self) -> PathBuf {
        self.data.join("minecraft")
    }
    fn server_jar(&self) -> PathBuf {
        self.data
            .join("server-jars")
            .join(format!("{}.jar", horizon_core::VANILLA_VERSION))
    }
}

/// The environment every `java` the preparation starts runs in.
pub fn java_environment(paths: &Paths) -> Vec<(&'static str, String)> {
    let jre = paths.jre.display();
    vec![
        ("HORIZON_JAVA", paths.java().display().to_string()),
        (
            "HORIZON_JAVAC",
            paths.libs.join("libhorizonjavac.so").display().to_string(),
        ),
        ("JAVA_HOME", jre.to_string()),
        (
            "LD_LIBRARY_PATH",
            format!("{jre}/lib/server:{jre}/lib:{}", paths.libs.display()),
        ),
        (
            "JDK_JAVA_OPTIONS",
            format!(
                "-Djava.io.tmpdir={} -Duser.home={} -Djava.awt.headless=true",
                paths.cache.display(),
                paths.data.display()
            ),
        ),
        ("TMPDIR", paths.cache.display().to_string()),
        ("HOME", paths.data.display().to_string()),
    ]
}

/// A step on its own thread: the last line it logged, and how it ended.
#[derive(Clone, Default)]
pub struct Progress {
    pub line: String,
    pub done: Option<Result<(), String>>,
}

pub struct Task {
    pub what: &'static str,
    state: Arc<Mutex<Progress>>,
}

impl Task {
    pub fn spawn(
        what: &'static str,
        work: impl FnOnce(&dyn Fn(String)) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let state = Arc::new(Mutex::new(Progress::default()));
        let thread_state = state.clone();
        std::thread::spawn(move || {
            let log = |line: String| {
                log::info!("Horizon launcher: {line}");
                thread_state.lock().unwrap().line = line;
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&log)))
                .unwrap_or_else(|_| Err("The launcher's worker stopped unexpectedly".into()));
            if let Err(e) = &result {
                log::warn!("Horizon launcher: {what} failed: {e}");
            }
            thread_state.lock().unwrap().done = Some(result);
        });
        Self { what, state }
    }
    pub fn progress(&self) -> Progress {
        self.state.lock().unwrap().clone()
    }
}

/// A download, tried three times: a phone's connection drops more often than a desktop's.
fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(2 * attempt));
        }
        match fetch_once(url) {
            Ok(body) => return Ok(body),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn fetch_once(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(&format!("horizon-android/{}", crate::backend::VERSION))
        .build()
        .get(url)
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("{url}: HTTP {code}"),
            ureq::Error::Transport(t) => format!("Could not download {url}: {t}"),
        })?;
    let mut body = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    Ok(body)
}

/// Unpacks `name` from the app's assets into `dir` when the copy there is
/// from another build of the app.
fn unpack(app: &AndroidApp, name: &str, dir: &Path, log: &dyn Fn(String)) -> Result<(), String> {
    let stamp = dir.join(".unpacked");
    let build = super::BUILD.to_owned();
    if fs::read_to_string(&stamp).is_ok_and(|s| s == build) {
        return Ok(());
    }
    log(format!("Unpacking {name}"));
    let partial = dir.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
    let file = std::ffi::CString::new(name).unwrap();
    let asset = app
        .asset_manager()
        .open(&file)
        .ok_or_else(|| format!("This build of the app has no {name}"))?;
    let mut zip = zip::ZipArchive::new(asset).map_err(|e| format!("{name}: {e}"))?;
    zip.extract(&partial).map_err(|e| format!("{name}: {e}"))?;
    // The preparation's `.local` lives in the version folder: keep it.
    let local = dir.join(".local");
    if local.is_dir() {
        let _ = fs::rename(&local, partial.join(".local"));
    }
    let _ = fs::remove_dir_all(dir);
    fs::rename(&partial, dir).map_err(|e| e.to_string())?;
    fs::write(&stamp, build).map_err(|e| e.to_string())
}

/// The app's own files: Horizon's version folder and the Java runtime.
pub fn unpack_app(app: &AndroidApp, paths: &Paths, log: &dyn Fn(String)) -> Result<(), String> {
    unpack(app, "horizon.zip", &paths.root, log)?;
    unpack(app, "jre-25.zip", &paths.jre, log)
}

fn pins(paths: &Paths) -> Result<sources::Pins, String> {
    let lock = fs::read_to_string(paths.root.join("research-lock.json")).map_err(|e| e.to_string())?;
    let lock: serde_json::Value = serde_json::from_str(&lock).map_err(|e| e.to_string())?;
    sources::Pins::read(&lock).ok_or_else(|| "research-lock.json doesn't pin the Minecraft files".into())
}

/// The Minecraft folder holding the pinned version: a copied one, or the
/// app's own download.
pub fn minecraft(paths: &Paths) -> Option<PathBuf> {
    let pins = pins(paths).ok()?;
    [paths.copied_minecraft(), paths.downloaded_minecraft()]
        .into_iter()
        .find(|dir| sources::find(dir, &pins).is_ok())
}

/// Downloads the pinned version from Mojang for the signed-in account, which
/// must own Minecraft: Java Edition.
fn download(paths: &Paths, pins: &sources::Pins, log: &dyn Fn(String)) -> Result<PathBuf, String> {
    let v = &pins.version;
    let ep = account::Endpoints::live();
    let mut accounts = account::Accounts::load();
    let Some((a, secrets)) = account::session_for_launch(&ep, account::store(), &mut accounts)?
    else {
        return Err(format!(
            "Minecraft {v} isn't on this phone yet. Sign in with the Microsoft account that owns \
             Minecraft: Java Edition and Horizon downloads it from Mojang, or copy a Minecraft \
             folder that has {v} to {}.",
            paths.copied_minecraft().display()
        ));
    };
    if !account::owns_minecraft(&ep, &secrets)? {
        return Err(format!(
            "{}, the signed-in account, doesn't own Minecraft: Java Edition, so Horizon can't \
             download it.",
            a.name
        ));
    }
    log(format!("{} owns Minecraft: downloading {v} from Mojang", a.name));
    let dir = paths.downloaded_minecraft();
    mojang::download(pins, &dir, "linux", 8, &fetch, log)?;
    Ok(dir)
}

fn server_jar(paths: &Paths, minecraft: &Path, log: &dyn Fn(String)) -> Result<PathBuf, String> {
    let jar = paths.server_jar();
    if jar.is_file() {
        return Ok(jar);
    }
    // One copied over with the Minecraft folder.
    let copied = minecraft.join("server.jar");
    if copied.is_file() {
        return Ok(copied);
    }
    let v = horizon_core::VANILLA_VERSION;
    let manifest = minecraft.join("versions").join(v).join(format!("{v}.json"));
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let url = json["downloads"]["server"]["url"]
        .as_str()
        .ok_or_else(|| format!("{} has no server download", manifest.display()))?;
    log(format!("Downloading the {v} server jar from Mojang"));
    let body = fetch(url)?;
    fs::create_dir_all(jar.parent().unwrap()).map_err(|e| e.to_string())?;
    let partial = jar.with_extension("partial");
    fs::write(&partial, body)
        .and_then(|_| fs::rename(&partial, &jar))
        .map_err(|e| e.to_string())?;
    Ok(jar)
}

/// Everything from the app's own files to prepared resources with sounds.
pub fn install(app: &AndroidApp, paths: &Paths, log: &dyn Fn(String)) -> Result<(), String> {
    unpack_app(app, paths, log)?;
    fs::create_dir_all(&paths.cache).map_err(|e| e.to_string())?;
    let pins = pins(paths)?;
    let minecraft = match minecraft(paths) {
        Some(dir) => dir,
        None => download(paths, &pins, log)?,
    };
    let reused = horizon_prepare::artifacts::reuse_imported(&paths.root, &|line| log(line.into()))?;
    let jar = if reused {
        None
    } else {
        Some(server_jar(paths, &minecraft, log)?)
    };
    let options = horizon_prepare::Options {
        root: paths.root.clone(),
        import: !reused,
        minecraft_dir: Some(minecraft.clone()),
        server_jar: jar,
        java_home: Some(paths.jre.clone()),
    };
    log("Preparing Horizon's resources (this takes a few minutes)".into());
    horizon_prepare::prepare(&options, &|line| log(line.into()))?;
    log("Copying the sounds".into());
    if let Err(e) = horizon_prepare::sounds(&paths.root, Some(&minecraft), &|line| log(line.into())) {
        log(format!("The game will be silent: {e}"));
    }
    fs::write(
        paths.root.join(".local").join("prepared-by-app"),
        super::BUILD,
    )
    .map_err(|e| e.to_string())?;
    log("Ready".into());
    Ok(())
}

/// The game's arguments for Play: the resources and the signed-in account.
pub fn game_args(paths: &Paths) -> Result<Vec<String>, String> {
    let mut args = vec![
        "--resources".to_owned(),
        paths.resources().display().to_string(),
    ];
    let mut accounts = account::Accounts::load();
    if account::client_id().is_some()
        && let Some((a, secrets)) =
            account::session_for_launch(&account::Endpoints::live(), account::store(), &mut accounts)?
    {
        let file = account::write_session_file(&paths.home.join("sessions"), &a, &secrets)?;
        args.push("--session-file".into());
        args.push(file.display().to_string());
    }
    Ok(args)
}
