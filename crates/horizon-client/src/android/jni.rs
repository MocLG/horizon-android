//! The activity's Java side (`android/app/.../HorizonActivity.java`): one
//! `platform(op, arg)` method for everything the NDK has no API for, and the
//! soft keyboard's text coming back through two native methods.
#![allow(unsafe_code)]
use jni::objects::{JClass, JObject, JString};
use jni::{EnvUnowned, JValue, jni_sig, jni_str};
use std::collections::VecDeque;
use std::sync::Mutex;

/// What the soft keyboard typed, in order, until the game takes it.
#[derive(Debug, PartialEq)]
pub enum Typed {
    Text(String),
    Backspace,
    Enter,
    /// The back gesture.
    Back,
}

static TYPED: Mutex<VecDeque<Typed>> = Mutex::new(VecDeque::new());
static WAKE: Mutex<Option<winit::event_loop::EventLoopProxy<()>>> = Mutex::new(None);

/// Wake the event loop when the keyboard types, so a paused frame notices.
pub fn set_wake(proxy: winit::event_loop::EventLoopProxy<()>) {
    *WAKE.lock().unwrap() = Some(proxy);
}

pub fn take_typed() -> Vec<Typed> {
    TYPED.lock().unwrap().drain(..).collect()
}

fn push(typed: Typed) {
    TYPED.lock().unwrap().push_back(typed);
    if let Some(proxy) = WAKE.lock().unwrap().as_ref() {
        let _ = proxy.send_event(());
    }
}

#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeText<'caller>(
    mut env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
) {
    env.with_env(|env| -> Result<(), jni::errors::Error> {
        push(Typed::Text(text.try_to_string(env)?));
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeKey<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    key: jni::sys::jint,
) {
    match key {
        1 => push(Typed::Backspace),
        2 => push(Typed::Enter),
        3 => push(Typed::Back),
        _ => {}
    }
}

static GYRO: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));

#[unsafe(no_mangle)]
extern "system" fn Java_com_lukagejak_horizon_HorizonActivity_nativeGyro<'caller>(
    _env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {
    let mut gyro = GYRO.lock().unwrap();
    gyro.0 += x;
    gyro.1 += y;
}

/// How far the phone turned, in radians, since the last call.
pub fn take_gyro() -> (f32, f32) {
    std::mem::take(&mut *GYRO.lock().unwrap())
}

pub fn gyroscope(on: bool) {
    platform("gyroscope", if on { "1" } else { "0" });
}

/// Calls `HorizonActivity.platform(op, arg)`; `None` when it returned null or failed.
pub fn platform(op: &str, arg: &str) -> Option<String> {
    let context = ndk_context::android_context();
    // SAFETY: android-activity put the process's JavaVM and the activity in the
    // context before android_main, and both live as long as the process.
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) };
    let activity = context.context() as jni::sys::jobject;
    let result = vm.attach_current_thread(|env| -> jni::errors::Result<Option<String>> {
        // SAFETY: a global reference to the activity, valid for the call.
        let activity = unsafe { env.as_cast_raw::<JObject<'static>>(&activity) }?;
        let op = JObject::from(env.new_string(op)?);
        let arg = JObject::from(env.new_string(arg)?);
        let out = env
            .call_method(
                &*activity,
                jni_str!("platform"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&op), JValue::Object(&arg)],
            )?
            .l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = env.cast_local::<JString>(out)?;
        Ok(Some(out.try_to_string(env)?))
    });
    result.unwrap_or_else(|e| {
        log::warn!("Horizon: Android {op} failed: {e}");
        None
    })
}

pub fn open_url(url: &str) {
    platform("open_url", url);
}

pub fn show_keyboard(show: bool) {
    platform("keyboard", if show { "1" } else { "0" });
}

pub fn vibrate(millis: u32) {
    platform("vibrate", &millis.to_string());
}

/// The game's clipboard is the activity's.
pub fn install_clipboard() {
    horizon_client::clipboard::install_android(horizon_client::clipboard::AndroidClipboard {
        get: || platform("clipboard_get", ""),
        set: |text| {
            platform("clipboard_set", text);
        },
    });
}
