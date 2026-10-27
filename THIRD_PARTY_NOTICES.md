# Third-party notices

## wgpu-hal

`third_party/wgpu-hal` is [wgpu-hal](https://github.com/gfx-rs/wgpu) 30.0.1 by the gfx-rs
developers, with one upstream fix applied
([gfx-rs/wgpu#10124](https://github.com/gfx-rs/wgpu/pull/10124)). It is licensed under MIT or
Apache-2.0; both licenses are in that folder (`LICENSE.MIT`, `LICENSE.APACHE`).

## Pixelify Sans

`crates/horizon-theme/fonts/PixelifySans-*.ttf` is [Pixelify Sans](https://github.com/eifetx/Pixelify-Sans)
by The Pixelify Sans Project Authors, licensed under the SIL Open Font License 1.1
(`crates/horizon-theme/fonts/OFL-PixelifySans.txt`).

## Pixelarticons

`crates/horizon-theme/icons/pixel/*.svg` are icons from
[Pixelarticons](https://github.com/halfmage/pixelarticons) by Gerrit Halfmann, under the MIT
license (`crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt`). `star.svg` is Horizon's own, drawn on
the same grid.

## Zalith Launcher 2

`crates/horizon-client/src/controls/default_layout.json` is the default control layout of
[Zalith Launcher 2](https://github.com/ZalithLauncher/ZalithLauncher2) by MovTery and
contributors (its `default_layout.json`, with its own name and author changed), licensed under
the GNU General Public License 3.0. Horizon reads and writes its layout format so that layouts
move between the two; none of Zalith's code is in Horizon.

The Android app (`android/build.sh`) carries the OpenJDK 25 runtime for Android that Zalith
Launcher 2 ships, built from OpenJDK, which is licensed under the GNU General Public License 2.0
with the Classpath Exception. It runs only the preparation's exporters, as a separate program.
