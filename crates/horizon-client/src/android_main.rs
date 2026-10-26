//! Horizon on Android: the library the app's activity loads (`android/`). The
//! same game as `main.rs`, behind a launcher drawn in the game's window.
#![deny(unsafe_code)]
mod android;
// The desktop's own start (`run`) and what only it uses stay unused here.
#[allow(dead_code)]
mod game;

// The desktop launcher's own code for accounts and Mojang's files, shared as
// it is; the parts only the desktop calls stay unused here.
#[allow(dead_code)]
#[path = "../../../launcher/src/account.rs"]
mod account;
#[allow(dead_code)]
#[path = "../../../launcher/src/mojang.rs"]
mod mojang;
#[allow(dead_code)]
#[path = "../../../launcher/src/sources.rs"]
mod sources;
use android::backend;

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android::main(app);
}
