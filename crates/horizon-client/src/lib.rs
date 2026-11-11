//! Gameplay and startup, above the reusable client library.
// No `#![recursion_limit]` or other inner attribute horizon-mixins doesn't know here: the
// launcher's horizon-mixins then rejects every native mod targeting this crate (#8169).
#![cfg_attr(not(target_os = "android"), forbid(unsafe_code))]
// The Android app (`android/`) calls the activity and the C library.
#![cfg_attr(target_os = "android", deny(unsafe_code))]
extern crate self as horizon_client;
pub use horizon_client_lib::*;
#[cfg(target_os = "android")]
pub mod android;
pub mod entry;
pub mod game;
mod horizon_look;
pub mod mod_api;

#[cfg(test)]
mod brewing_speed_load_tests;
#[cfg(test)]
mod living_death_tests;

// The desktop launcher's own code for accounts and Mojang's files, shared as
// it is, with the few things it takes from the launcher around it.
#[cfg(target_os = "android")]
#[allow(dead_code)]
#[path = "../../../launcher/src/account.rs"]
mod account;
#[cfg(target_os = "android")]
#[allow(dead_code)]
#[path = "../../../launcher/src/mojang.rs"]
mod mojang;
#[cfg(target_os = "android")]
#[allow(dead_code)]
#[path = "../../../launcher/src/sources.rs"]
mod sources;
#[cfg(target_os = "android")]
use android::backend;

/// The desktop launcher's proxy setting lives in its own settings, which the
/// Android launcher has none of: requests go direct.
#[cfg(target_os = "android")]
mod proxy {
    pub fn agent(builder: ureq::AgentBuilder, _url: &str) -> ureq::AgentBuilder {
        builder
    }
}

/// The launcher's words are English here.
#[cfg(target_os = "android")]
mod i18n {
    pub fn tr(en: &str) -> &str {
        en
    }
    pub fn trf(en: &str, args: &[(&str, &str)]) -> String {
        let mut out = en.to_string();
        for (name, value) in args {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        out
    }
}

/// The player the launcher hands the real game, which the app never starts.
#[cfg(target_os = "android")]
mod java_launch {
    #[allow(dead_code)]
    pub struct Player {
        pub name: String,
        pub uuid: String,
        pub token: String,
        pub xuid: String,
    }
}
