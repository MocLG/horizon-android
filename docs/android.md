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

Touch works as in Zalith Launcher 2, with its control layouts
(`crates/horizon-client/src/controls.rs`). The default layout is Zalith's:
GUI shows and hides the others, Esc, F3, F5, Tab, T and IME along the top,
E above the hotbar, PRI and SEC (left and right click, which also let the
finger look around), sneak as a toggle, jump, Ctrl and Alt, and the
joystick, which locks forward and sprints when pushed past its top.

- In the world a finger on no control looks around. On the hotbar a tap
  selects a slot, a double tap swaps hands and a long press drops.
- Over a screen a finger moves the mouse pointer like a touchpad: a tap
  clicks where it points and a long press holds the button.
- The back gesture does what Escape does. The soft keyboard opens with chat,
  signs, books and the other text fields, or with the IME button.
- The ball over the game (drag it anywhere) opens the game menu: the layout
  editor, gestures (a tap uses, a hold breaks), the gyroscope (turning the
  phone looks around), the controls' opacity and size, a larger interface,
  hiding the controls, the keyboard and pause.
- Controls size Auto shrinks a layout made for a wider screen until its
  buttons stop overlapping, in steps of 5%. The larger interface lets the
  GUI scale go past vanilla's largest (the GUI keeps 160 GUI pixels of
  height instead of 240), with GUI Scale on Auto or set higher.
- The layout editor shows every layer. Drag a widget to move it, tap it for
  its panel: label, size, the keys it sends, toggle, swipe, look through,
  where it shows, its style, duplicate and delete. The toolbar adds buttons,
  joysticks and layers and saves the layout as a file of its own.
- A gamepad plays with Horizon's controller support, and layers set to hide
  for one do while it is in use.

Layouts exported from Zalith Launcher 2 load as they are. Copy their `.json`
files to `Android/data/com.lukagejak.horizon/files/controls` and press
Import in Settings, then pick one with Next. Export writes the current
layout there for Zalith or another phone. Settings also turns the touch
controls off for a keyboard or a controller.

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

`HORIZON_ANDROID_PROFILE=android` builds the game without LTO, which
links in a fraction of the time, and `HORIZON_ANDROID_JOBS` sets cargo's
jobs: 4 by default, and 1 on a phone, where more run out of memory.

`cargo check -p horizon-client --example horizon --target
aarch64-linux-android --no-default-features --features audio,android`
after sourcing `android/tools/env.sh` checks the Android code alone.

## Updates

`android/release.sh` publishes the APK the last build made as a GitHub
release of MocLG/horizon-android, tagged `android-<versionCode>`. The
launcher looks for the latest release at start, and when its version code
is higher than its own it shows Update: the APK downloads into the app's
cache folder and Android's installer opens on it. Android installs it only
when the release key signed both builds, so publish only builds signed
with it.

## Logs

`adb logcat -s Horizon` shows the game's and the launcher's log, panics
included. The game also writes `logs/latest.log` and crash reports in its
folder, as on the desktop.
