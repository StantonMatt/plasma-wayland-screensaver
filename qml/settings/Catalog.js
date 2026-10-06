// SPDX-License-Identifier: GPL-3.0-or-later
// What the settings window offers: animations in gallery order, backgrounds,
// palettes, and the label each animation uses for the four shared sliders.
// An empty label hides that slider for the animation.
.pragma library
.import "../visuals/VisualUtils.js" as VisualUtils

var snakeWorldEventsAvailable = true

var animations = [
    { id: "none", name: qsTr("None") },
    { id: "aurora", name: qsTr("Aurora Drift"),
      speed: qsTr("Speed"), density: qsTr("Layers"), scale: qsTr("Size"), trail: qsTr("Glow") },
    { id: "orbs", name: qsTr("Floating Orbs"),
      speed: qsTr("Speed"), density: qsTr("Orbs"), scale: qsTr("Size"), trail: qsTr("Glow") },
    { id: "bounce", name: qsTr("Bouncing Balls"),
      speed: qsTr("Speed"), density: "", scale: qsTr("Ball size"), trail: qsTr("Trails") },
    { id: "starfield", name: qsTr("Hyperspace"),
      speed: qsTr("Warp speed"), density: qsTr("Stars"), scale: qsTr("Star size"), trail: qsTr("Trails") },
    { id: "matrix", name: qsTr("Digital Rain"),
      speed: qsTr("Fall speed"), density: qsTr("Density"), scale: qsTr("Glyph size"), trail: qsTr("Glow") },
    { id: "kaleidoscope", name: qsTr("Kaleidoscope"),
      speed: qsTr("Speed"), density: qsTr("Detail"), scale: qsTr("Spread"), trail: qsTr("Glow") },
    { id: "fireflies", name: qsTr("Fireflies"),
      speed: qsTr("Speed"), density: qsTr("Swarm size"), scale: qsTr("Size"), trail: qsTr("Glow") },
    { id: "ribbons", name: qsTr("Neon Ribbons"),
      speed: qsTr("Speed"), density: qsTr("Ribbons"), scale: qsTr("Thickness"), trail: qsTr("Glow") },
    { id: "constellation", name: qsTr("Constellation"),
      speed: qsTr("Speed"), density: qsTr("Stars"), scale: qsTr("Star size"), trail: qsTr("Links") },
    { id: "snakes", name: qsTr("Slithering Snakes"),
      speed: qsTr("Speed"), density: qsTr("Population"), scale: qsTr("Thickness"), trail: qsTr("Food") }
]

// Same gradients as Screensaver.qml's backgroundTop()/backgroundBottom().
var backgrounds = [
    { value: "black", name: qsTr("Pure Black"), top: "#000000", bottom: "#000000" },
    { value: "midnight", name: qsTr("Midnight"), top: "#071329", bottom: "#13091e" },
    { value: "ocean", name: qsTr("Deep Ocean"), top: "#02111d", bottom: "#062f3c" },
    { value: "plum", name: qsTr("Dark Plum"), top: "#18091d", bottom: "#310d2c" }
]

var palettes = [
    { value: "ocean", name: qsTr("Ocean Electric"), colors: VisualUtils.colors("ocean") },
    { value: "spectrum", name: qsTr("Full Spectrum"), colors: VisualUtils.colors("spectrum") },
    { value: "ember", name: qsTr("Ember & Gold"), colors: VisualUtils.colors("ember") },
    { value: "forest", name: qsTr("Forest Glow"), colors: VisualUtils.colors("forest") },
    { value: "mono", name: qsTr("Monochrome"), colors: VisualUtils.colors("mono") },
    { value: "pastel", name: qsTr("Soft Pastels"), colors: VisualUtils.colors("pastel") }
]

function animationIndex(id) {
    for (var i = 0; i < animations.length; ++i)
        if (animations[i].id === id) return i
    return -1
}

function animation(id) {
    var index = animationIndex(id)
    return animations[index < 0 ? 1 : index]
}

function background(value) {
    for (var i = 0; i < backgrounds.length; ++i)
        if (backgrounds[i].value === value) return backgrounds[i]
    return backgrounds[1]
}
