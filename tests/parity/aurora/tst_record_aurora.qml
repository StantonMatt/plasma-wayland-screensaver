// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "oracle/VisualUtils.js" as Utils

TestCase {
    id: testCase
    name: "AuroraRecorder"
    width: 640
    height: 360
    visible: true
    when: windowShown
    QtObject {
        id: settings
        property real animationDensity: 55
        property real animationScale: 200
        property real animationSpeed: 300
        property real trailAmount: 100
        property string animationPalette: "ocean"
        property var presentationClock: null
    }
    function test_record() {
        const request = new XMLHttpRequest()
        const url = Qt.resolvedUrl("oracle/Aurora.qml")
        request.open("GET", url, false)
        request.send()
        let source = request.responseText.replace('const ctx = getContext("2d")', 'const ctx = recorderContext')
                                         .replace('onPaint: {', 'function recordPaint() {')
        const end = source.lastIndexOf('}')
        source = source.slice(0, end) + '\nproperty var recorderContext\n' + source.slice(end)
        const visual = Qt.createQmlObject(source, testCase, url)
        const palettes = ["ocean", "spectrum", "ember", "forest", "mono", "pastel"]
        const scenarios = [[55,2,3,1,1,3440,1440,0], [10,.25,.1,0,-1197,640,360,1.2345],
                           [100,2,3,1,429,1920,1080,12.5], [50,1,1,.35,1,2560,1440,100000],
                           [55,2,3,1,1,3440,1440,12.5]]
        for (let p = 0; p < palettes.length; ++p) {
            for (let s = 0; s < scenarios.length; ++s) {
                for (let reduced = 0; reduced <= 1; ++reduced) {
                    const c = scenarios[s]
                    settings.animationDensity=c[0]; settings.animationScale=c[1]*100
                    settings.animationSpeed=c[2]*100; settings.trailAmount=c[3]*100
                    settings.animationPalette=palettes[p]
                    visual.context=settings; visual.seed=c[4]; visual.width=c[5]; visual.height=c[6]
                    visual.reducedMotion=!!reduced
                    visual.phase=(c[4]%1000)/159 + (reduced ? 0 : c[7]*.54*c[2])
                    let bands=[]; let shade=[]; let current=null
                    const ctx = {globalAlpha:1, reset:function(){}, clearRect:function(){},
                        createRadialGradient:function(x,y,r0,x1,y1,r) {
                            current={geometry:[x,y,r],stops:[]}; bands.push(current)
                            return {addColorStop:function(t,color) {current.stops.push([t,color])}}
                        },
                        createLinearGradient:function() {
                            current={stops:[]}; shade=current.stops
                            return {addColorStop:function(t,color) {current.stops.push([t,color])}}
                        },
                        fillRect:function(){current.alpha=this.globalAlpha}}
                    visual.recorderContext=ctx
                    visual.recordPaint()
                    console.log("AURORA_FIXTURE " + JSON.stringify({config:c,palette:p,reduced:reduced,
                        phase:visual.phase,bands:bands,shade:shade,shadeAlpha:current.alpha}))
                }
            }
        }
        for (let seed of [-1197,1,429,2147483647]) {
            for (let i of [-2,0,1,123])
                console.log("AURORA_RANDOM " + JSON.stringify([i,seed,Utils.random(i,seed)]))
        }
        visual.destroy()
    }
}
