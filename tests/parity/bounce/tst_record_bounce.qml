// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
TestCase {
    id: testCase
    name: "BounceRecorder"
    width: 640; height: 360
    visible: true; when: windowShown
    QtObject {
        id: settings
        property int ballCount: 20
        property real animationScale: 200
        property real animationSpeed: 300
        property real trailAmount: 100
        property real ballGravity: 10
        property real ballElasticity: 100
        property bool ballCollisions: true
        property string animationPalette: "ocean"
        property string monitorBehavior: "independent"
        property var animationState: null
        property var presentationClock: null
    }
    function test_record() {
        const url=Qt.resolvedUrl("oracle/Bounce.qml")
        const request=new XMLHttpRequest();request.open("GET",url,false);request.send()
        let source=request.responseText.replace('const painter = getContext("2d")','const painter = root.recorderContext')
            .replace('onPaint: {','function recordPaint() {')
        const end=source.lastIndexOf('}')
        source=source.slice(0,end)+'\nproperty var recorderContext\nfunction renderRecords(ctx) { recorderContext=ctx; localCanvas.recordPaint() }\n'+source.slice(end)
        // owner, min/max signed gravity, elasticity extremes, collisions on/off,
        // tiny viewport, negative/large seeds and trail lengths 0/3/8.
        const scenarios=[[3440,1440,1,20,2,3,1,10,100,1],
            [3440,1440,1,20,2,3,1,10,100,0],
            [640,360,-1197,20,.25,.1,0,-100,50,0],
            [640,360,429,20,2,3,1,100,50,1],
            [1920,1080,2147483647,1,1,1,.35,0,100,0],
            [320,180,1,20,2,3,1,-100,100,1],
            [640,360,1,20,1,1,.35,100,50,0],
            [640,360,1,20,1,1,.35,-100,50,1]]
        const frames=[0,1,2,30,60,120,300,600]
        for(let c=0;c<scenarios.length;++c) {
            const s=scenarios[c]
            settings.ballCount=s[3];settings.animationScale=s[4]*100;settings.animationSpeed=s[5]*100
            settings.trailAmount=s[6]*100;settings.ballGravity=s[7];settings.ballElasticity=s[8];settings.ballCollisions=!!s[9]
            settings.animationPalette=c%2===0?"ocean":"ember"
            const visual=Qt.createQmlObject(source,testCase,url)
            visual.width=s[0];visual.height=s[1];visual.seed=s[2];visual.context=settings;visual.reducedMotion=true
            verify(visual!==null);visual.initializeBalls()
            let prior=0
            for(const frame of frames) {
                for(let step=prior;step<frame;++step) visual.stepPhysics(1/60)
                prior=frame
                let records=[];let arc=[]
                const ctx={globalAlpha:1,fillStyle:"",reset:function(){this.globalAlpha=1},clearRect:function(){},
                    scale:function(){},beginPath:function(){},arc:function(x,y,r){arc=[x,y,r]},
                    createRadialGradient:function(x0,y0,r0,x1,y1,r1){return {geometry:[x0,y0,r0,x1,y1,r1],colors:[],
                        addColorStop:function(offset,color){this.colors.push([offset,color])}}},
                    fill:function(){
                        const gradient=typeof this.fillStyle!=="string"
                        let color=this.fillStyle;let extra=[0,0,0,0]
                        if(gradient){
                            const g=this.fillStyle;compare(g.colors[0][0],0);compare(g.colors[0][1],"#ffffff")
                            compare(g.colors[1][0],.22);compare(g.colors[2][0],1);compare(g.colors[2][1],"#131b45")
                            color=g.colors[1][1];extra=[g.geometry[0],g.geometry[1],g.geometry[2],g.geometry[5]]
                        }
                        records.push([arc[0],arc[1],1,1,parseInt(color.slice(1),16),this.globalAlpha,arc[2],gradient?3:1,...extra])
                    }}
                visual.renderRecords(ctx)
                console.log("BOUNCE_FRAME "+JSON.stringify([c,frame,...s,c%2===0?0:2,records.length]))
                for(let i=0;i<visual.localBalls.length;++i) {
                    const b=visual.localBalls[i];let trail=[]
                    for(const t of b.trail)trail.push(t.x,t.y)
                    console.log("BOUNCE_BALL "+JSON.stringify([i,b.x,b.y,b.vx,b.vy,b.size,b.trail.length,...trail]))
                }
                for(let i=0;i<records.length;++i)console.log("BOUNCE_PRIMITIVE "+JSON.stringify([i,...records[i]]))
            }
            visual.destroy()
        }
    }
}
