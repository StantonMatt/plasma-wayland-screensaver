// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "oracle/VisualUtils.js" as Utils
TestCase {
    id: testCase
    name: "MatrixRecorder"
    width: 640; height: 360
    visible: true; when: windowShown
    QtObject {
        id: settings
        property real animationDensity: 55
        property real animationScale: 200
        property real animationSpeed: 300
        property real trailAmount: 100
        property string animationPalette: "ocean"
        property var presentationClock: null
    }
    Text { id: fontProbe; text: "0"; font.family: "monospace"; visible: false }
    function test_record() {
        const url=Qt.resolvedUrl("oracle/Matrix.qml")
        const request=new XMLHttpRequest();request.open("GET",url,false);request.send()
        let source=request.responseText.replace("Repeater {","Repeater {\n id: recorderColumns")
        source=source.replace("text: stream.text.charAt",'objectName: "matrixHead"; text: stream.text.charAt')
        const end=source.lastIndexOf('}')
        source=source.slice(0,end)+'\nfunction columnAt(i) {return recorderColumns.itemAt(i)}\n'+source.slice(end)
        const visual=Qt.createQmlObject(source,testCase,url)
        const palettes=["ocean","spectrum","ember","forest","mono","pastel"]
        const scenarios=[[55,2,3,1,1,3440,1440,0],[55,2,3,1,1,3440,1440,12.5],
            [10,.25,.1,0,-1197,640,360,-1.2345],[100,2,3,1,429,1920,1080,12.5],
            [50,1,1,.35,1,2560,1440,100000],[50,1,1,.35,2147483647,320,180,.333333333],
            [55,2,3,1,1,3440,1440,1e-8],[55,2,3,1,1,3440,1440,1000000]]
        fontProbe.font.pixelSize=47
        const glyphSize=47,lineHeight=glyphSize*.95,streamHeight=7*lineHeight+Math.trunc(fontProbe.contentHeight-lineHeight)
        const wrap=(1-Utils.random(3,1))*(1440+streamHeight)/(.18*3*(.32+Utils.random(2,1)*.82)*1440)
        scenarios.push([55,2,3,1,1,3440,1440,wrap-1e-8],[55,2,3,1,1,3440,1440,wrap+1e-8])
        for(let p=0;p<palettes.length;++p) for(const c of scenarios) for(let reduced=0;reduced<=1;++reduced) {
            settings.animationDensity=c[0];settings.animationScale=c[1]*100;settings.animationSpeed=c[2]*100
            settings.trailAmount=c[3]*100;settings.animationPalette=palettes[p]
            visual.context=settings;visual.seed=c[4];visual.width=c[5];visual.height=c[6];visual.reducedMotion=!!reduced
            visual.phase=reduced?0:c[7]*.18*c[2]
            fontProbe.font.pixelSize=visual.glyphSize
            console.log("MATRIX_FRAME "+JSON.stringify([...c,p,reduced,visual.phase,visual.glyphSize,visual.columnWidth,visual.columnCount,fontProbe.contentHeight,.28+visual.glow*.52,.72+visual.glow*.28]))
            for(let i=0;i<visual.columnCount;++i){const col=visual.columnAt(i);const text=col.children[0];const head=findChild(col,"matrixHead")
                console.log("MATRIX_COLUMN "+JSON.stringify([i,col.x,col.y,col.streamLength,col.fallRate,col.startOffset,text.contentHeight,head.y,text.color.toString(),text.text.split("\n").map(x=>visual.glyphs.indexOf(x))]))
            }
        }
        visual.destroy()
    }
}
