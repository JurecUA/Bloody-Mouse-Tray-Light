# Bloody Mouse Tray Light

[English] | [Русский](README.ru.md) | [Українська](README.uk.md)

Native Rust edition of Bloody Mouse Tray for Windows. It displays the Bloody SG5 Pro mouse battery charge, polling rate, and DPI in the system tray, with portable settings and Russian, Ukrainian, and English interface languages.

Settings and full-charge history are kept in `mouse_tray.json` next to the executable. The app works without Bloody Central and can optionally start with Windows.

To try it without building, run [`release/Bloody Mouse Tray Light.exe`](release/Bloody%20Mouse%20Tray%20Light.exe). Keep [`release/mouse_tray.json`](release/mouse_tray.json) beside it for the included Russian-language defaults.

## Build from source

On 64-bit Windows, install the Rust MSVC toolchain and Visual Studio C++ Build Tools, then run:

```powershell
cargo build --release
```

The release executable is `target/release/bloody-mouse-tray-light.exe`.
