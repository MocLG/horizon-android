//! The launcher, as the desktop one looks (`launcher/`, `docs/design-system.md`):
//! the Play page with the account, the game files and Play, and Settings.
//! Drawn in the game's window before the game takes it over.
use super::draw::{Face, Rect, Renderer, hex};
use super::install::{self, Paths, Task};
use super::jni;
use super::controls;
use super::ui::{CONTROL, GUTTER, Pointer, ROW, Ui};
use super::update::{self, Release};
use crate::account;
use horizon_client::graphics::{self, GraphicsApi, Preset};
use horizon_client::settings::Settings;
use horizon_client::ui_theme::preference;
use horizon_theme::DARK as P;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::{TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::platform::android::activity::AndroidApp;
use winit::window::Window;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Play,
    Settings,
}

/// A text field the soft keyboard types into: the client ID.
struct Editing {
    text: String,
}

pub struct Launcher {
    app: AndroidApp,
    pub paths: Paths,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pointer: Pointer,
    page: Page,
    scroll: f32,
    task: Option<Task>,
    /// The last task's failure, until another starts.
    error: Option<String>,
    sign_in: Option<account::SignIn>,
    /// The sign-in's code was opened in the browser already.
    opened_code: bool,
    accounts: account::Accounts,
    editing: Option<Editing>,
    /// The touch controls, kept in the game's horizon.txt.
    touch: bool,
    /// The game settings the next start reads.
    settings: Settings,
    worker_priority: bool,
    sustained: bool,
    game_mode: String,
    started: Instant,
    /// The game's last crash, from its crash-reports folder.
    crash: Option<String>,
    /// The display cutout's margins in pixels, asked again when the window changes.
    insets: Option<[f32; 4]>,
    /// Where Minecraft is, and when that was last looked at: finding it
    /// hashes the client jar, too slow for every frame.
    minecraft: Option<(Option<std::path::PathBuf>, Instant)>,
    /// A newer release of the app, once the check at start found one.
    update: Arc<Mutex<Option<Release>>>,
    /// Set by Play: the arguments the game starts with.
    pub play: Option<Vec<String>>,
}

/// The display cutout's margins in pixels: left, top, right, bottom.
fn insets() -> [f32; 4] {
    let mut insets = [0.0; 4];
    if let Some(text) = jni::platform("insets", "") {
        for (inset, value) in insets.iter_mut().zip(text.split(',')) {
            *inset = value.parse().unwrap_or(0.0);
        }
    }
    insets
}

impl Launcher {
    pub fn new(app: AndroidApp, paths: Paths, crash: Option<String>) -> Self {
        let touch = preference(&paths.home, "touchControls").is_none_or(|v| v != "false");
        let worker_priority =
            preference(&paths.home, "workerPriority").is_none_or(|v| v != "false");
        let mut sustained =
            preference(&paths.home, "sustainedPerformance").is_some_and(|v| v == "true");
        let mut settings = Settings::load(&paths.home.join("options.txt"));
        let game_mode = jni::platform("game_mode", "").unwrap_or_else(|| "unsupported".into());
        // Adapt to Android GameManager mode if active
        if game_mode == "battery" && !settings.horizon.battery {
            settings.horizon.battery = true;
            settings.max_fps = settings.max_fps.min(60);
        } else if game_mode == "performance" && !sustained {
            sustained = true;
        }
        let mut launcher = Self {
            app,
            paths,
            window: None,
            renderer: None,
            pointer: Pointer::default(),
            page: Page::Play,
            scroll: 0.0,
            task: None,
            error: None,
            sign_in: None,
            opened_code: false,
            accounts: account::Accounts::load(),
            editing: None,
            touch,
            settings,
            worker_priority,
            sustained,
            game_mode,
            started: Instant::now(),
            crash,
            insets: None,
            minecraft: None,
            update: update::check(),
            play: None,
        };
        // The app's own files first, so the version folder is there to read.
        launcher.run("Unpacking", |app, paths, log| install::unpack_app(app, paths, log));
        launcher
    }

    fn run(
        &mut self,
        what: &'static str,
        work: impl FnOnce(&AndroidApp, &Paths, &dyn Fn(String)) -> Result<(), String> + Send + 'static,
    ) {
        let (app, paths) = (self.app.clone(), self.paths.clone());
        self.error = None;
        self.task = Some(Task::spawn(what, move |log| work(&app, &paths, log)));
    }

    fn busy(&self) -> bool {
        self.task.is_some()
    }

    pub fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match event_loop.create_window(Window::default_attributes().with_title("Horizon")) {
                Ok(w) => self.window = Some(Arc::new(w)),
                Err(e) => {
                    log::error!("Horizon launcher: no window: {e}");
                    return;
                }
            }
        }
        if self.renderer.is_none()
            && let Some(window) = &self.window
        {
            match Renderer::new(window.clone()) {
                Ok(r) => self.renderer = Some(r),
                Err(e) => log::error!("Horizon launcher: no renderer: {e}"),
            }
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The window is going away (the app left the screen): the surface goes with it.
    pub fn suspended(&mut self) {
        self.renderer = None;
    }

    /// The window and renderer go to the game: the surface must be released first.
    pub fn hand_over(&mut self) -> Option<Arc<Window>> {
        self.renderer = None;
        jni::show_keyboard(false);
        self.window.take()
    }

    pub fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub fn window_event(&mut self, event: WindowEvent) {
        let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
        match event {
            WindowEvent::Resized(size) => {
                self.insets = None;
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height, scale);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(r), Some(w)) = (&mut self.renderer, &self.window) {
                    let size = w.inner_size();
                    r.resize(size.width, size.height, scale_factor as f32);
                    w.request_redraw();
                }
            }
            WindowEvent::Touch(touch) => {
                let at = (
                    touch.location.x as f32 / scale,
                    touch.location.y as f32 / scale,
                );
                match touch.phase {
                    TouchPhase::Started => self.pointer.press(at),
                    TouchPhase::Moved => self.pointer.moved(at),
                    TouchPhase::Ended => self.pointer.release(),
                    TouchPhase::Cancelled => self.pointer.cancel(),
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    /// A game that could not start.
    pub fn failed(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Keyboard text for the field being edited, and the back gesture.
    fn typed(&mut self) -> bool {
        let typed = jni::take_typed();
        if typed.is_empty() {
            return false;
        }
        if typed.contains(&jni::Typed::Back) {
            if let Some(s) = self.sign_in.take() {
                s.cancel();
            } else if self.editing.take().is_some() {
                jni::show_keyboard(false);
            } else {
                self.page = Page::Play;
                self.scroll = 0.0;
            }
            return true;
        }
        let Some(editing) = &mut self.editing else {
            return false;
        };
        for t in typed {
            match t {
                jni::Typed::Back => {}
                jni::Typed::Text(text) => editing.text.push_str(&text),
                jni::Typed::Backspace => {
                    editing.text.pop();
                }
                jni::Typed::Enter => {
                    let id = editing.text.trim().to_owned();
                    self.save_client_id(&id);
                    self.editing = None;
                    jni::show_keyboard(false);
                    return true;
                }
            }
        }
        true
    }

    fn save_client_id(&mut self, id: &str) {
        let path = crate::backend::data_dir().join("client-id.txt");
        let _ = std::fs::create_dir_all(crate::backend::data_dir());
        let result = if id.is_empty() {
            std::fs::remove_file(&path).or(Ok(()))
        } else {
            std::fs::write(&path, id)
        };
        if let Err(e) = result {
            self.error = Some(format!("The client ID could not be saved: {e}"));
        }
    }

    pub fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut dirty = self.typed();
        if let Some(task) = &self.task {
            dirty = true;
            if let Some(done) = task.progress().done {
                if let Err(e) = done {
                    self.error = Some(e);
                }
                self.task = None;
                self.minecraft = None;
            }
        }
        if let Some(sign_in) = &self.sign_in {
            dirty = true;
            match sign_in.state() {
                account::SignInState::Code(code) if !self.opened_code => {
                    self.opened_code = true;
                    jni::open_url(&format!(
                        "{}?otc={}",
                        code.verification_uri, code.user_code
                    ));
                }
                account::SignInState::Done(result) => {
                    if let Err(e) = result {
                        self.error = Some(e);
                    }
                    self.sign_in = None;
                    self.accounts = account::Accounts::load();
                }
                _ => {}
            }
        }
        let busy = self.busy() || self.sign_in.is_some();
        if busy {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(33),
            ));
        } else {
            if dirty {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn draw(&mut self) {
        let Some(mut renderer) = self.renderer.take() else {
            return;
        };
        if self.insets.is_none() {
            self.insets = Some(insets());
        }
        let insets = self.insets.unwrap_or_default();
        let inset = |i: usize| insets[i] / renderer.scale;
        let margins = (inset(0).max(GUTTER), inset(2).max(GUTTER));
        let pointer = std::mem::take(&mut self.pointer.tap);
        let scroll = std::mem::take(&mut self.pointer.scroll);
        self.scroll = (self.scroll + scroll).min(0.0);
        let tap_pointer = Pointer {
            down: self.pointer.down,
            tap: pointer,
            scroll: 0.0,
        };
        let result = renderer.frame(hex(P.page, 1.0), |c| {
            let mut ui = Ui::new(c, &tap_pointer);
            self.layout(&mut ui, margins);
        });
        if let Err(e) = result {
            log::warn!("Horizon launcher: frame failed: {e}");
        }
        self.renderer = Some(renderer);
    }

    fn layout(&mut self, ui: &mut Ui, (left, right): (f32, f32)) {
        let (w, h) = (ui.c.width, ui.c.height);
        // The signature: the wordmark, giant and faint, at the foot.
        let size = (w * 0.24).min(220.0);
        let mark_w = ui.c.measure("Horizon", Face::Medium, size);
        ui.text(
            (w - mark_w) / 2.0,
            h - size * 0.82,
            "Horizon",
            Face::Medium,
            size,
            hex(P.plate, 1.0),
        );
        let content = Rect::new(left, 0.0, w - left - right, h);
        let top = 64.0;
        let page_h = match self.page {
            Page::Play => self.play_page(ui, content, top + self.scroll),
            Page::Settings => self.settings_page(ui, content, top + self.scroll),
        };
        // Scrolling stops at the end of the page.
        let overflow = (page_h - h + 24.0).max(0.0);
        self.scroll = self.scroll.max(-overflow);
        // The bar over the page.
        ui.c.rect(Rect::new(0.0, 0.0, w, 56.0), 0.0, hex(P.page, 1.0));
        ui.text(left, 18.0, "Horizon", Face::Medium, 17.0, hex(P.ink, 1.0));
        let mut x = content.right();
        for (page, label) in [(Page::Settings, "Settings"), (Page::Play, "Play")] {
            let tw = ui.c.measure(label, Face::Medium, horizon_theme::UI.px) + 28.0;
            x -= tw;
            let r = Rect::new(x, 12.0, tw, CONTROL);
            if self.page == page {
                ui.c.rect(r, r.h / 2.0, hex(P.plate, 1.0));
            }
            ui.centred(
                r,
                label,
                Face::Medium,
                horizon_theme::UI.px,
                hex(if self.page == page { P.ink } else { P.muted }, 1.0),
            );
            if ui.tapped(r) && self.page != page {
                self.page = page;
                self.scroll = 0.0;
            }
            x -= 4.0;
        }
        if let Some(sign_in) = &self.sign_in {
            let state = sign_in.state();
            self.sign_in_dialog(ui, state);
        }
    }

    fn status(&self) -> &'static str {
        if self.paths.prepared() {
            "Ready"
        } else {
            "Needs preparing"
        }
    }

    fn signed_in(&self) -> Option<&account::Account> {
        self.accounts.selected()
    }

    /// Returns the page's height.
    fn play_page(&mut self, ui: &mut Ui, r: Rect, mut y: f32) -> f32 {
        let caption = format!("Minecraft {} · {}", horizon_core::VANILLA_VERSION, self.status());
        ui.text(r.x, y, &caption, Face::Regular, 12.0, hex(P.muted, 1.0));
        y += 20.0;
        let display = 44.0;
        ui.text(r.x, y, "Horizon", Face::Medium, display, hex(P.ink, 1.0));
        // Play, the page's one main action.
        let play = Rect::new(r.right() - 148.0, y + 4.0, 148.0, 44.0);
        let ready = self.paths.prepared() && !self.busy();
        if ui.button(play, "Play", true, ready) {
            self.save_performance();
            self.apply_android_hints();
            match install::game_args(&self.paths) {
                Ok(args) => self.play = Some(args),
                Err(e) => self.error = Some(e),
            }
        }
        y += ui.c.line(Face::Medium, display) + 6.0;
        let about = format!(
            "{} for Android · Minecraft: Java Edition {}, rewritten in Rust and checked against the game itself.",
            super::BUILD,
            horizon_core::VANILLA_VERSION
        );
        for line in ui.c.wrap(&about, Face::Regular, 14.0, r.w.min(620.0)) {
            ui.text(r.x, y, &line, Face::Regular, 14.0, hex(P.ink_soft, 1.0));
            y += 21.0;
        }
        y += 18.0;
        ui.c.rule(r.x, y, r.w, hex(P.rule, 1.0));
        if let Some(crash) = self.crash.clone() {
            let row = Rect::new(r.x, y, r.w, ROW);
            ui.c.rect(
                Rect::new(row.x - 10.0, row.y + row.h / 2.0 - 3.0, 6.0, 6.0),
                3.0,
                hex(P.danger, 1.0),
            );
            ui.row(row, 0, "The game crashed last time", &crash, 90.0);
            if ui.row_button(row, "Dismiss", false, true) {
                self.crash = None;
            }
            y += ROW;
        }
        let release = self.update.lock().unwrap().clone();
        if let Some(release) = release {
            let row = Rect::new(r.x, y, r.w, ROW);
            let size = format!("{} MB", release.size >> 20);
            ui.row(
                row,
                0,
                &format!("{} is out", release.name),
                &format!("A {size} download. Your worlds, settings and accounts stay."),
                110.0,
            );
            if ui.row_button(row, "Update", true, !self.busy()) {
                self.run("Updating", move |_, paths, log| update::install(&release, paths, log));
            }
            y += ROW;
        }
        // 01 who plays.
        let row = Rect::new(r.x, y, r.w, ROW);
        let (name, detail) = match self.signed_in() {
            Some(a) => (format!("Playing as {}", a.name), "Your Microsoft account. Online-mode servers know you by it.".to_owned()),
            None => (
                "Not signed in".to_owned(),
                "Sign in with the Microsoft account that owns Minecraft: Java Edition. Horizon downloads the game files with it.".to_owned(),
            ),
        };
        ui.row(row, 1, &name, &detail, 110.0);
        if self.signed_in().is_none()
            && ui.row_button(row, "Sign in", false, self.sign_in.is_none())
        {
            self.start_sign_in();
        }
        y += ROW;
        // 02 the game files.
        let row = Rect::new(r.x, y, r.w, ROW);
        let detail = match self.minecraft() {
            Some(dir) if dir == self.paths.copied_minecraft() => "From the Minecraft folder you copied to this phone.".to_owned(),
            Some(_) => "Downloaded from Mojang for your account.".to_owned(),
            None => format!(
                "Not on this phone yet. Signing in downloads it, or copy a Minecraft folder with {} to {}.",
                horizon_core::VANILLA_VERSION,
                self.paths.copied_minecraft().display()
            ),
        };
        ui.row(row, 2, &format!("Minecraft {}", horizon_core::VANILLA_VERSION), &detail, 110.0);
        y += ROW;
        // 03 the resources, prepared from them.
        let row = Rect::new(r.x, y, r.w, ROW);
        let (detail, label) = if self.paths.prepared() {
            ("Prepared. Horizon built them from your own Minecraft.", "Prepare again")
        } else {
            (
                "Not prepared. Horizon builds them from the files of your own Minecraft, once.",
                "Install",
            )
        };
        let wide = ui.button_width(label);
        ui.row(row, 3, "Resources", detail, wide);
        if ui.row_button(row, label, !self.paths.prepared(), !self.busy()) {
            self.prepare();
        }
        y += ROW;
        y = self.task_and_error(ui, r, y + 12.0);
        y - self.scroll
    }

    /// The Minecraft folder, looked for again every few seconds and after a task.
    fn minecraft(&mut self) -> Option<std::path::PathBuf> {
        let stale = self.minecraft.as_ref().is_none_or(|(_, at)| {
            at.elapsed() > std::time::Duration::from_secs(5)
        });
        if stale && !self.busy() {
            self.minecraft = Some((install::minecraft(&self.paths), Instant::now()));
        }
        self.minecraft.as_ref().and_then(|(dir, _)| dir.clone())
    }

    fn prepare(&mut self) {
        let _ = std::fs::remove_file(self.paths.root.join(".local").join("prepared-by-app"));
        self.run("Installing", |app, paths, log| install::install(app, paths, log));
    }

    fn task_and_error(&mut self, ui: &mut Ui, r: Rect, mut y: f32) -> f32 {
        if let Some(task) = &self.task {
            let progress = task.progress();
            ui.text(r.x, y, task.what, Face::Medium, 14.0, hex(P.ink, 1.0));
            y += 22.0;
            let line = ui.c.fit(&progress.line, Face::Mono, 12.0, r.w);
            ui.text(r.x, y, &line, Face::Mono, 12.0, hex(P.muted, 1.0));
            y += 20.0;
            let t = self.started.elapsed().as_secs_f32();
            ui.progress(Rect::new(r.x, y, r.w, 3.0), None, t);
            y += 16.0;
        }
        if let Some(error) = self.error.clone() {
            for line in ui.c.wrap(&error, Face::Regular, 13.0, r.w) {
                ui.text(r.x, y, &line, Face::Regular, 13.0, hex(P.danger, 1.0));
                y += 19.0;
            }
            y += 8.0;
        }
        y
    }

    fn start_sign_in(&mut self) {
        if account::client_id().is_none() {
            self.error = Some(format!(
                "{} Set an Azure client ID in Settings to sign in.",
                account::NOT_CONFIGURED
            ));
            return;
        }
        self.opened_code = false;
        self.error = None;
        self.sign_in = Some(account::SignIn::start());
    }

    fn sign_in_dialog(&mut self, ui: &mut Ui, state: account::SignInState) {
        let (w, h) = (ui.c.width, ui.c.height);
        ui.c.rect(Rect::new(0.0, 0.0, w, h), 0.0, hex(P.page, 0.85));
        let d = Rect::new((w - 420.0) / 2.0, (h - 250.0) / 2.0, 420.0, 250.0);
        ui.c.rect(d, horizon_theme::RADIUS_PLATE, hex(P.surface, 1.0));
        let x = d.x + 24.0;
        let mut y = d.y + 22.0;
        ui.text(x, y, "Sign in with Microsoft", Face::Medium, 20.0, hex(P.ink, 1.0));
        y += 34.0;
        let code = match &state {
            account::SignInState::Code(code) => Some(code.clone()),
            _ => None,
        };
        match &code {
            Some(code) => {
                let text = format!("Enter this code at {}:", code.verification_uri);
                ui.text(x, y, &text, Face::Regular, 13.0, hex(P.muted, 1.0));
                y += 26.0;
                ui.text(x, y, &code.user_code, Face::Mono, 30.0, hex(P.ink, 1.0));
                y += 54.0;
            }
            None => {
                ui.text(x, y, "Asking Microsoft for a code…", Face::Regular, 13.0, hex(P.muted, 1.0));
                y += 80.0;
            }
        }
        let mut bx = x;
        if let Some(code) = &code {
            let label = "Open the page";
            let bw = ui.button_width(label);
            if ui.button(Rect::new(bx, y, bw, CONTROL), label, true, true) {
                jni::open_url(&format!("{}?otc={}", code.verification_uri, code.user_code));
            }
            bx += bw + 8.0;
            let label = "Copy code";
            let bw = ui.button_width(label);
            if ui.button(Rect::new(bx, y, bw, CONTROL), label, false, true) {
                jni::platform("clipboard_set", &code.user_code);
            }
            bx += bw + 8.0;
        }
        let label = "Cancel";
        let bw = ui.button_width(label);
        if ui.button(Rect::new(bx, y, bw, CONTROL), label, false, true) {
            if let Some(s) = &self.sign_in {
                s.cancel();
            }
            self.sign_in = None;
        }
    }

    fn save_performance(&mut self) {
        let _ = std::fs::write(
            self.paths.home.join("options.txt"),
            self.settings.serialize(),
        );
        self.settings.horizon.save(&self.paths.home);
        set_preference(
            &self.paths.home,
            "workerPriority",
            if self.worker_priority {
                "true"
            } else {
                "false"
            },
        );
        set_preference(
            &self.paths.home,
            "sustainedPerformance",
            if self.sustained { "true" } else { "false" },
        );
    }

    fn apply_android_hints(&mut self) {
        jni::platform(
            "sustained_performance",
            if self.sustained { "1" } else { "0" },
        );
        self.game_mode = jni::platform("game_mode", "").unwrap_or_else(|| "unsupported".into());
        if self.game_mode == "battery" && !self.settings.horizon.battery {
            self.settings.horizon.battery = true;
            self.settings.max_fps = self.settings.max_fps.min(60);
            self.save_performance();
        }
    }

    fn settings_page(&mut self, ui: &mut Ui, r: Rect, mut y: f32) -> f32 {
        ui.text(r.x, y, "Settings", Face::Medium, 44.0, hex(P.ink, 1.0));
        y += ui.c.line(Face::Medium, 44.0) + 4.0;
        ui.text(
            r.x,
            y,
            "Your account, the controls and the game's files.",
            Face::Regular,
            14.0,
            hex(P.muted, 1.0),
        );
        y += 34.0;
        ui.c.rule(r.x, y, r.w, hex(P.rule, 1.0));
        let mut index = 0;
        let mut next = |y: &mut f32| {
            index += 1;
            let row = Rect::new(r.x, *y, r.w, ROW);
            *y += ROW;
            (row, index)
        };
        // Microsoft account.
        let (row, i) = next(&mut y);
        match self.signed_in().cloned() {
            Some(a) => {
                ui.row(row, i, "Microsoft account", &format!("Signed in as {}.", a.name), 110.0);
                if ui.row_button(row, "Sign out", false, true) {
                    let mut accounts = account::Accounts::load();
                    if let Err(e) = accounts.remove(account::store(), &a.uuid).and_then(|_| accounts.save()) {
                        self.error = Some(e);
                    }
                    self.accounts = account::Accounts::load();
                }
            }
            None => {
                ui.row(row, i, "Microsoft account", "Not signed in: the game plays offline.", 110.0);
                if ui.row_button(row, "Sign in", false, self.sign_in.is_none()) {
                    self.start_sign_in();
                }
            }
        }
        // The Azure app the sign-in uses.
        let (row, i) = next(&mut y);
        let editing = self.editing.is_some();
        let detail = match (&self.editing, account::client_id()) {
            (Some(e), _) => format!("{}|", e.text),
            (None, Some(id)) => id,
            (None, None) => "Not set. Sign-in needs the client ID of an Azure app Mojang approved.".into(),
        };
        ui.row(row, i, "Azure client ID", &detail, 90.0);
        if editing {
            if ui.row_button(row, "Save", true, true) {
                let id = self.editing.take().map(|e| e.text).unwrap_or_default();
                self.save_client_id(id.trim());
                jni::show_keyboard(false);
            }
        } else if ui.row_button(row, "Edit", false, true) {
            self.editing = Some(Editing {
                text: account::client_id().unwrap_or_default(),
            });
            jni::take_typed();
            jni::show_keyboard(true);
        }
        // Touch controls.
        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "Touch controls",
            "Buttons and a joystick over the game, as in Zalith Launcher 2. Off with a controller or keyboard.",
            48.0,
        );
        if let Some(on) = ui.switch(row, self.touch) {
            self.touch = on;
            set_preference(&self.paths.home, "touchControls", if on { "true" } else { "false" });
        }
        // The control layout, and Zalith Launcher 2's layouts in and out.
        let (row, i) = next(&mut y);
        let layouts = controls::layouts(&self.paths.home);
        let current = controls::selected(&self.paths.home);
        let index = layouts.iter().position(|(file, _)| *file == current).unwrap_or(0);
        let name = layouts.get(index).map_or("Default", |(_, name)| name.as_str());
        ui.row(
            row,
            i,
            "Control layout",
            &format!("{name}, {} of {}.", index + 1, layouts.len()),
            90.0,
        );
        if ui.row_button(row, "Next", false, layouts.len() > 1) {
            let (file, _) = &layouts[(index + 1) % layouts.len()];
            set_preference(&self.paths.home, "controlLayout", file);
        }
        let (row, i) = next(&mut y);
        let folder = self.paths.shared.join("controls");
        ui.row(
            row,
            i,
            "Import and export layouts",
            &format!(
                "Zalith Launcher 2's layouts work as they are: copy their .json files to {}, then Import. Export puts the current layout there.",
                folder.display()
            ),
            190.0,
        );
        let export = ui.button_width("Export");
        let import_rect = Rect::new(row.right() - export - 8.0 - ui.button_width("Import"), row.y + (row.h - CONTROL) / 2.0, ui.button_width("Import"), CONTROL);
        if ui.button(import_rect, "Import", false, true) {
            self.error = Some(match controls::import(&folder, &self.paths.home) {
                Ok(0) => format!("No layouts to import in {}.", folder.display()),
                Ok(n) => format!("Imported {n} control layouts."),
                Err(e) => e,
            });
        }
        if ui.row_button(row, "Export", false, true) {
            self.error = Some(match controls::export(&self.paths.home, &folder) {
                Ok(file) => format!("Exported to {}.", file.display()),
                Err(e) => e,
            });
        }

        let (row, i) = next(&mut y);
        let preset = self.settings.horizon.preset;
        let control = ui.button_width("Performance");
        ui.row(
            row,
            i,
            "Graphics preset",
            "Sets distance, clouds, particles, shadows and mipmaps together.",
            control,
        );
        if ui.row_button(row, preset.label(), false, true) {
            let preset = match self.settings.horizon.preset {
                Preset::Performance => Preset::Balanced,
                Preset::Balanced => Preset::Quality,
                Preset::Quality => Preset::Ultra,
                Preset::Ultra => Preset::Performance,
                Preset::Custom => Preset::Performance,
            };
            graphics::apply_preset(preset, &mut self.settings);
            self.settings.horizon.preset = preset;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let scales = [50, 60, 70, 80, 90, 100];
        let scale = self.settings.horizon.resolution_scale;
        let control = ui.button_width("100%");
        ui.row(
            row,
            i,
            "Resolution scale",
            "World pixels per screen pixel; below 100% is faster on the GPU.",
            control,
        );
        if ui.row_button(
            row,
            &format!("{}%", scale),
            false,
            true,
        ) {
            let next = scales
                .into_iter()
                .find(|s| *s > scale)
                .unwrap_or(scales[0]);
            self.settings.horizon.resolution_scale = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let framerates = [30, 45, 60, 90, 120, 144, 165, 260];
        let fps = self.settings.max_fps;
        let control = ui.button_width("Unlimited");
        ui.row(
            row,
            i,
            "Maximum FPS",
            "A cap saves power and heat; unlimited is 260.",
            control,
        );
        if ui.row_button(row, &format!("{} FPS", fps), false, true) {
            let next = framerates
                .into_iter()
                .find(|f| *f > fps)
                .unwrap_or(framerates[0]);
            self.settings.max_fps = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "VSync",
            "Waits for the display: no tearing, a little more latency.",
            40.0,
        );
        if let Some(on) = ui.switch(row, self.settings.vsync) {
            self.settings.vsync = on;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let distances = [4, 6, 8, 10, 12, 16, 24, 32];
        let rd = self.settings.render_distance;
        let control = ui.button_width("32 chunks");
        ui.row(
            row,
            i,
            "Render distance",
            "Full-detail terrain around you, in chunks.",
            control,
        );
        if ui.row_button(row, &format!("{} chunks", rd), false, true) {
            let next = distances
                .into_iter()
                .find(|d| *d > rd)
                .unwrap_or(distances[0]);
            self.settings.render_distance = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let distances = [5, 6, 8, 10, 12, 16, 24, 32];
        let sim = self.settings.simulation_distance;
        let control = ui.button_width("32 chunks");
        ui.row(
            row,
            i,
            "Simulation distance",
            "How far blocks, mobs and random ticks update, in chunks.",
            control,
        );
        if ui.row_button(row, &format!("{} chunks", sim), false, true) {
            let next = distances
                .into_iter()
                .find(|d| *d > sim)
                .unwrap_or(distances[0]);
            self.settings.simulation_distance = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let workers = [1, 2, 3, 4, 6, 8];
        let value = if self.settings.horizon.workers == 0 {
            format!("Auto ({})", self.settings.horizon.worker_threads())
        } else {
            self.settings.horizon.workers.to_string()
        };
        let control = ui.button_width("Auto (00)");
        ui.row(
            row,
            i,
            "Worker threads",
            "Chunk generation, light and meshing. Auto is every core but one; fewer leave headroom.",
            control,
        );
        if ui.row_button(row, &value, false, true) {
            let next = workers
                .into_iter()
                .find(|t| *t > self.settings.horizon.workers)
                .unwrap_or(workers[0]);
            self.settings.horizon.workers = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "Worker priority",
            "Chunk workers run ten nice levels below the render and server threads.",
            40.0,
        );
        if let Some(on) = ui.switch(row, self.worker_priority) {
            self.worker_priority = on;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let budgets = [0, 256, 512, 1024, 2048, 4096, 8192, 16384];
        let budget = self.settings.horizon.upload_budget_kib;
        let value = match budget {
            0 => "Unlimited".to_owned(),
            n if n >= 1024 => format!("{} MiB", n / 1024),
            n => format!("{n} KiB"),
        };
        let control = ui.button_width("16 MiB");
        ui.row(
            row,
            i,
            "Upload budget",
            "Mesh data sent to the GPU each frame. Unlimited is fastest with headroom.",
            control,
        );
        if ui.row_button(row, &value, false, true) {
            let next = budgets
                .into_iter()
                .find(|b| *b > budget)
                .unwrap_or(budgets[0]);
            self.settings.horizon.upload_budget_kib = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "Battery mode",
            "Caps the game at 60 FPS (30 in menus) and halves the worker threads.",
            40.0,
        );
        if let Some(on) = ui.switch(row, self.settings.horizon.battery) {
            self.settings.horizon.battery = on;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        let api = match self.settings.horizon.api {
            GraphicsApi::Auto | GraphicsApi::Vulkan | GraphicsApi::OpenGl => self.settings.horizon.api,
            _ => GraphicsApi::Auto,
        };
        let control = ui.button_width("Vulkan");
        ui.row(
            row,
            i,
            "Graphics API",
            "Vulkan is the fast path; OpenGL is for drivers without it.",
            control,
        );
        if ui.row_button(row, api.label(), false, true) {
            let next = match api {
                GraphicsApi::Auto => GraphicsApi::Vulkan,
                GraphicsApi::Vulkan => GraphicsApi::OpenGl,
                GraphicsApi::OpenGl => GraphicsApi::Auto,
                _ => GraphicsApi::Auto,
            };
            self.settings.horizon.api = next;
            self.save_performance();
        }

        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "Sustained performance",
            "Asks Android to hold clocks steady for long sessions instead of boosting.",
            40.0,
        );
        if let Some(on) = ui.switch(row, self.sustained) {
            self.sustained = on;
            self.save_performance();
            jni::platform("sustained_performance", if on { "1" } else { "0" });
        }

        if self.game_mode != "unsupported" {
            let (row, i) = next(&mut y);
            let mode = match self.game_mode.as_str() {
                "performance" => "Performance",
                "battery" => "Battery",
                "standard" => "Standard",
                "custom" => "Custom",
                _ => "Not supported",
            };
            let control = ui.button_width("Refresh");
            ui.row(
                row,
                i,
                "Android game mode",
                &format!(
                    "Android reports {mode}. Chosen in the Game Dashboard; Horizon reads the hint."
                ),
                control,
            );
            if ui.row_button(row, "Refresh", false, true) {
                self.game_mode =
                    jni::platform("game_mode", "").unwrap_or_else(|| "unsupported".into());
                if self.game_mode == "battery" && !self.settings.horizon.battery {
                    self.settings.horizon.battery = true;
                    self.settings.max_fps = self.settings.max_fps.min(60);
                    self.save_performance();
                } else if self.game_mode == "performance" && !self.sustained {
                    self.sustained = true;
                    self.save_performance();
                    jni::platform("sustained_performance", "1");
                }
            }
        }
        // Where a Minecraft folder can be copied to.
        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            "Minecraft folder",
            &format!(
                "To play without signing in, copy a Minecraft folder with {} to {} over USB.",
                horizon_core::VANILLA_VERSION,
                self.paths.copied_minecraft().display()
            ),
            0.0,
        );
        // Preparing again.
        let (row, i) = next(&mut y);
        ui.row(row, i, "Resources", "Prepare them again from your Minecraft, for example after it changed.", 140.0);
        if ui.row_button(row, "Prepare again", false, !self.busy()) {
            self.prepare();
            self.page = Page::Play;
            self.scroll = 0.0;
        }
        // About.
        let (row, i) = next(&mut y);
        ui.row(
            row,
            i,
            &format!("Horizon {}", super::BUILD),
            "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.",
            0.0,
        );
        y = self.task_and_error(ui, r, y + 12.0);
        y - self.scroll
    }
}

/// One `key:value` line of the game's horizon.txt (`ui_theme::preference`).
fn set_preference(dir: &std::path::Path, key: &str, value: &str) {
    let path = dir.join("horizon.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once(':').is_none_or(|(k, _)| k.trim() != key))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{key}:{value}"));
    let _ = std::fs::write(path, lines.join("\n") + "\n");
}
