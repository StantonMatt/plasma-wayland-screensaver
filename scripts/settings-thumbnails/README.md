# Settings gallery thumbnails

Regenerate on a machine where a private D-Bus and headless Wayland compositor
can create sockets:

```bash
scripts/settings-thumbnails/thumbnails.sh \
  "$PWD/build/bin/plasma-visual-screensaver" "$PWD/qml/images/thumbnails"
```

Requires `kwin-wayland`, `spectacle`, `dbus`, Python 3 and Pillow. The private
session bus has no service activation. Each visual uses a timed diagnostic
preview, independent of the desktop's running screensaver or inhibitors.
KWin, the preview and Spectacle are terminated and waited for on normal exit,
failure or interruption. Captures/logs use disk-backed agent scratch and are
removed when the run finishes; only short-lived socket directories use `/tmp`.

The ten initial PNGs are the designer's real 0.13.0 prototype captures, used as
placeholders pending regeneration from 0.14.0. The settings-redesign worker's
sandbox denied the private D-Bus socket (`Operation not permitted`), before
starting KWin. The documented surfaceless EGL renderer can capture Snake
vertex dumps, but does not render the other nine QML visuals.

Output is 320×180 RGBA. Alpha is the brightest original channel; RGB is
unpremultiplied, so compositing over a selected background retains each visual's
colors. `None` needs no image. Runtime URLs are
`qrc:/qml/images/thumbnails/<visualModule>.png`.
