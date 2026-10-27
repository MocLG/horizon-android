//! The app's one event loop: the launcher until Play, then the game in the
//! same window. Android gives a process one event loop, so both live in it.
use super::launcher::Launcher;
use crate::game::android::AndroidGame;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

enum State {
    Launcher(Box<Launcher>),
    Game(Box<AndroidGame>),
}

pub struct Shell {
    state: State,
    /// How the game ended, for the next start's launcher.
    pub error: Option<Box<dyn std::error::Error>>,
}

impl Shell {
    pub fn new(launcher: Launcher) -> Self {
        Self {
            state: State::Launcher(Box::new(launcher)),
            error: None,
        }
    }

    fn play(&mut self, event_loop: &ActiveEventLoop) {
        let State::Launcher(launcher) = &mut self.state else {
            return;
        };
        let Some(args) = launcher.play.take() else {
            return;
        };
        log::info!("Horizon: starting the game");
        // The launcher's surface goes before the game makes its own on the window.
        drop(launcher.hand_over());
        match AndroidGame::start(args) {
            Ok(mut game) => {
                game.resumed(event_loop);
                self.state = State::Game(Box::new(game));
            }
            Err(e) => {
                log::error!("Horizon: the game could not start: {e}");
                launcher.failed(e.to_string());
                launcher.resumed(event_loop);
            }
        }
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.state {
            State::Launcher(l) => l.resumed(event_loop),
            State::Game(g) => g.resumed(event_loop),
        }
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.state {
            State::Launcher(l) => l.suspended(),
            State::Game(g) => g.suspended(event_loop),
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, (): ()) {
        if let State::Launcher(l) = &self.state {
            l.request_redraw();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &mut self.state {
            State::Launcher(l) => l.window_event(event),
            State::Game(g) => g.window_event(event_loop, id, event),
        }
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let State::Game(g) = &mut self.state {
            g.device_event(event_loop, id, event);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match &mut self.state {
            State::Launcher(l) => {
                l.about_to_wait(event_loop);
            }
            State::Game(g) => g.about_to_wait(event_loop),
        }
        self.play(event_loop);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let State::Game(g) = &mut self.state {
            g.exiting(event_loop);
            self.error = g.error();
        }
    }
}
