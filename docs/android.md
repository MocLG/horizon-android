# Horizon on Android

The Android app is the same game, built for aarch64 Android, behind a
launcher in the game's own window. It needs a phone or tablet with Vulkan
and Android 10 or newer.

## What the app does

1. **Launcher.** The Play page says who plays, whether Minecraft 26.3 is on
   the phone and whether the resources are prepared, as the desktop
   launcher's does. Settings has the Microsoft account, the Azure client ID,
   the touch controls and preparing again.
2. **Your own Minecraft.** Signing in with the Microsoft account that owns
   Minecraft: Java Edition lets the app download 26.3 from Mojang, with the
   desktop launcher's own code (`launcher/src/mojang.rs`). Without signing
   in, copy a Minecraft folder that has 26.3 (`versions/26.3`, `libraries`,
   `assets`) to `Android/data/com.lukagejak.horizon/files/minecraft` over
   USB.
3. **Preparing.** `horizon-prepare` runs in the app. Its exporters call
   Mojang's own code, so they run on a Java runtime the app carries: Zalith
   Launcher 2's OpenJDK 25 for Android, pinned in `android/build.sh`. The
   phone compiles the exporters too when the APK ships none that match.
4. **Play.** The launcher hands the window to the game, which starts on its
   title screen.

Nothing of Mojang's is in the APK.

## Controls

Over a screen a finger is the mouse: a tap clicks, a drag drags, a long
press is a right click and two fingers scroll.

In the world:

| Touch | Does |
|---|---|
| A thumb on the left | The stick appears under it: move. Pushed fully forward, sprint |
| A finger anywhere else | Look |
| Tap | Attack what the crosshair is on, else use the hand |
| Hold | Break, or use food, a bow or a spyglass while held |
| Jump, Sneak | Jump (twice to fly), sneak on and off |
| Items, Drop, Chat | E, Q and T |
| View, Pause | F5, and what Escape does |
| The hotbar | Select a slot |

The back gesture does what Escape does. The soft keyboard opens with chat,
signs, books and the other text fields. Settings turns the touch controls
off for a keyboard or a controller.

## How it is built

- `crates/horizon-client/src/android_main.rs` is the library's root: an
  example of `horizon-client` behind the `android` feature, so it compiles
  the game module (`game.rs`, `game/`) as `main.rs` does. `src/android/` is
  the launcher (wgpu, Geist, the design system) and the Java bridge.
  `src/game/android.rs` is the touch input, the keyboard, the back gesture
  and the window that goes away while the app is in the background.
- Android allows one event loop per process: the launcher and the game share
  it (`src/android/shell.rs`), and the process ends with the game.
- `android/app` is a NativeActivity whose Java side only reaches what has no
  native API: the clipboard, links, the soft keyboard, the back gesture.
- Android starts no program from an app's data folder, where the runtime is
  unpacked. `android/java-launcher` is `java` and `javac` as a small program
  packaged as a native library (`libhorizonjava.so`). It loads the runtime's
  `libjli.so` and calls `JLI_Launch`, as `bin/java` does.
  `horizon-prepare` takes it from `HORIZON_JAVA` and `HORIZON_JAVAC`.

## Build it

```sh
rustup target add aarch64-linux-android
android/build.sh
```

It needs clang and lld, the Android SDK and NDK (`ANDROID_HOME`), a JDK
for Gradle, xz, zip and python3. The NDK's own clang is an x86_64 program,
so `android/tools/ndk-clang` runs the host's clang against the NDK's
sysroot and any host can build the app. On an aarch64 host qemu runs the
SDK's aapt2.

The APK lands in `android/build/`. Set `HORIZON_ANDROID_KEYSTORE`,
`HORIZON_ANDROID_KEYSTORE_PASSWORD` and `HORIZON_ANDROID_KEY_ALIAS` to sign
it with your key, else Gradle's debug key signs it. `HORIZON_EXPORTERS`
names a release's `exporters` folder to ship when it was built from the
same sources.

`cargo check -p horizon-client --example horizon --target
aarch64-linux-android --no-default-features --features audio,android`
after sourcing `android/tools/env.sh` checks the Android code alone.

## Logs

`adb logcat -s Horizon` shows the game's and the launcher's log, panics
included. The game also writes `logs/latest.log` and crash reports in its
folder, as on the desktop.
