// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "oracle/VisualUtils.js" as Utils

TestCase {
    id: testCase
    name: "PrimitiveRecorder"
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
    function test_record() {
        const palettes=["ocean","spectrum","ember","forest","mono","pastel"]
        // Includes initial state, negative seed/phase, long-running phase, wrap
        // boundary samples, minimum and maximum settings, and owner settings.
        const scenarios=[[55,2,3,1,1,3440,1440,0], [10,.25,.1,0,-1197,640,360,-1.2345],
            [100,2,3,1,429,1920,1080,12.5], [50,1,1,.35,1,2560,1440,100000],
            [55,2,3,1,1,3440,1440,12.5], [50,1,1,.35,2147483647,320,180,0.333333333],
            [50,1,1,.35,1,320,180,0.333333334],
            [50,1,1,.35,1,320,180,(1-Utils.random(3,1))/.12-1e-8],
            [50,1,1,.35,1,320,180,(1-Utils.random(3,1))/.12+1e-8]]
        for (const name of ["Constellation","Starfield"]) {
            const url=Qt.resolvedUrl("oracle/"+name+".qml")
            const request=new XMLHttpRequest();request.open("GET",url,false);request.send()
            let source=request.responseText.replace('const painter = getContext("2d")','const painter = recorderContext')
                .replace('onPaint: {','function recordPaint() {')
            const end=source.lastIndexOf('}')
            source=source.slice(0,end)+'\nproperty var recorderContext\n'+source.slice(end)
            const visual=Qt.createQmlObject(source,testCase,url)
            for (let p=0;p<palettes.length;++p) for(let s=0;s<scenarios.length;++s) for(let reduced=0;reduced<=1;++reduced) {
                const c=scenarios[s];settings.animationDensity=c[0];settings.animationScale=c[1]*100
                settings.animationSpeed=c[2]*100;settings.trailAmount=c[3]*100;settings.animationPalette=palettes[p]
                visual.context=settings;visual.seed=c[4];visual.width=c[5];visual.height=c[6];visual.reducedMotion=!!reduced
                visual.phase=reduced?0:c[7]*(name==="Constellation"?.35:.12)
                let records=[];let start=[];let endPoint=[];let radial=[]
                const ctx={globalAlpha:1,lineWidth:1,strokeStyle:"",fillStyle:"",reset:function(){},clearRect:function(){},
                    beginPath:function(){},moveTo:function(x,y){start=[x,y]},lineTo:function(x,y){endPoint=[x,y]},
                    stroke:function(){records.push([...start,...endPoint,this.strokeStyle,this.globalAlpha,this.lineWidth/2,0])},
                    arc:function(x,y,r){start=[x,y,r]},fill:function(){records.push([start[0],start[1],0,0,this.fillStyle,this.globalAlpha,start[2],1])},
                    createRadialGradient:function(x,y,r0,x1,y1,r){radial=[x,y,r];return {addColorStop:function(t,color){if(t===0)radial.push(color);else compare(color,"#00000000")}}},
                    fillRect:function(){records.push([radial[0],radial[1],0,0,radial[3],this.globalAlpha,radial[2],2])}}
                visual.recorderContext=ctx;visual.recordPaint()
                console.log("PRIMITIVE_FRAME "+JSON.stringify([name,...c,p,reduced,visual.phase,records.length]))
                for(let i=0;i<records.length;++i)console.log("PRIMITIVE_RECORD "+JSON.stringify([i,...records[i]]))
            }
            visual.destroy()
        }
    }
}
