//! Horizon on Android: the library the app's activity loads (`android/`). The
//! same game as `main.rs`, behind a launcher drawn in the game's window.
#![deny(unsafe_code)]

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    horizon_client::android::main(app);
}
