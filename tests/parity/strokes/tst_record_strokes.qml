// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
TestCase {
    id: testCase
    name: "StrokesRecorder"
    width: 640; height: 360; visible: true; when: windowShown
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
        const scenarios=[[55,2,3,1,1,3440,1440,12.5], [10,.25,.1,0,-1197,640,360,1.2345],
            [100,2,3,1,429,1920,1080,12.5], [50,1,1,.35,1,2560,1440,100000],
            [55,2,3,.25,1,3440,1440,0], [55,2,3,.250001,1,3440,1440,12.5]]
        for (let name of ["Ribbons","Kaleidoscope"]) {
            const url=Qt.resolvedUrl("oracle/"+name+".qml")
            const request=new XMLHttpRequest();request.open("GET",url,false);request.send()
            let source=request.responseText.replace('const painter = getContext("2d")','const painter = recorderContext')
                .replace('onPaint: {','function recordPaint() {')
            const end=source.lastIndexOf('}')
            source=source.slice(0,end)+'\nproperty var recorderContext\n'+source.slice(end)
            const visual=Qt.createQmlObject(source,testCase,url)
            for (let p=0;p<palettes.length;++p) for (let c of scenarios) for (let reduced=0;reduced<=1;++reduced) {
                settings.animationDensity=c[0];settings.animationScale=c[1]*100;settings.animationSpeed=c[2]*100
                settings.trailAmount=c[3]*100;settings.animationPalette=palettes[p]
                visual.context=settings;visual.seed=c[4];visual.width=c[5];visual.height=c[6];visual.reducedMotion=!!reduced
                visual.phase=reduced?0:c[7]*(name==="Ribbons"?.55:1)
                let commands=[];let path=[];let stack=[];let angle=0;let origin=[0,0]
                function transform(x,y) {return [origin[0]+x*Math.cos(angle)-y*Math.sin(angle),origin[1]+x*Math.sin(angle)+y*Math.cos(angle)]}
                const ctx={globalAlpha:1,lineWidth:1,strokeStyle:"",fillStyle:"",
                    reset:function(){},clearRect:function(){},
                    save:function(){stack.push([angle,origin])},restore:function(){const v=stack.pop();angle=v[0];origin=v[1]},
                    translate:function(x,y){origin=transform(x,y)},rotate:function(a){angle+=a},
                    beginPath:function(){path=[]},moveTo:function(x,y){path.push(...transform(x,y))},
                    bezierCurveTo:function(a,b,c,d,e,f){path.push(...transform(a,b),...transform(c,d),...transform(e,f))},
                    quadraticCurveTo:function(a,b,c,d){path.push(...transform(a,b),...transform(c,d))},
                    arc:function(x,y,r,start,end){path.push(...transform(x,y),r)},
                    fill:function(){commands.push(["C",...path,this.globalAlpha,this.fillStyle])},
                    stroke:function(){commands.push([name==="Ribbons"?"R":"Q",...path,this.lineWidth,this.globalAlpha,this.strokeStyle])}}
                visual.recorderContext=ctx;visual.recordPaint()
                console.log("STROKES_FIXTURE "+JSON.stringify({name:name,config:c,palette:p,reduced:reduced,phase:visual.phase,commands:commands}))
            }
            visual.destroy()
        }
    }
}
