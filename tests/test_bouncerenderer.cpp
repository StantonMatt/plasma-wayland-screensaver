// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualrenderer.h"
#include "animationstate.h"
#include <QTest>
#include <QQuickWindow>
#include <QQmlEngine>
#include <QQmlComponent>
#include <QSignalSpy>
#include <QFile>
#include <QDir>
#include <QJsonObject>
#include <QJsonDocument>
#include <QPainter>
#include <QDateTime>
#include <cmath>
#include <cstring>
// These unchanged inputs are frozen at 8992f46. The name substitution lets the
// typed native sampler be checked against the old QVariant projection/history.
#define AnimationState FrozenAnimationState
#include "parity/bounce/oracle/animationstate.cpp"
#undef AnimationState
// MOC for FrozenAnimationState is compiled separately (CMake source header).
class BounceRendererTest final : public QObject {
    Q_OBJECT
    bool gpu() const {return qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU");}
    static void configure(VisualRenderer &r,int mode,int count=20,double scale=2,double speed=3,double trails=1,
        const QString &palette=QStringLiteral("ocean"),double gravity=10,double elasticity=100,bool collisions=true,bool reduced=false) {
        r.configureBounce(55,scale,speed,trails,palette,1,reduced,QDateTime::currentMSecsSinceEpoch(),count,gravity,elasticity,collisions,mode);
    }
    static void source(VisualRenderer &r,AnimationState &state) {
        r.setBounceSource([&state](qint64 ns,visuals_bounce_ball *out,size_t capacity) {
            std::array<AnimationState::BallSnapshot,20> balls;
            const auto n=state.ballSnapshotAt(ns,balls.data(),std::min(capacity,balls.size()));
            for(size_t i=0;i<n;++i)out[i]={balls[i].x,balls[i].y,balls[i].vx,balls[i].vy,balls[i].size};
            return n;
        });
    }
private Q_SLOTS:
    void fallbackCounts_data() {
        QTest::addColumn<int>("mode");
        QTest::addColumn<int>("count");
        for (int mode : {0, 1, 2}) for (int count : {20, 21, 60})
            QTest::newRow(qPrintable(QString::number(mode) + QStringLiteral("-") + QString::number(count))) << mode << count;
    }
    void fallbackCounts() {
        QFETCH(int, mode); QFETCH(int, count);
        AnimationState state;
        state.configureGeometries({{0, 0, 640, 360}}, true, false, QStringLiteral("normal"),
                                  60, 20, 300, 200, 10, 100, true, true);
        VisualRenderer renderer;
        renderer.setSize({640, 360});
        source(renderer, state);
        renderer.setViewport(state.bounds(), {});
        renderer.m_supportCheck = [](QQuickWindow*) { return false; };
        configure(renderer, mode, count);
        QVERIFY(renderer.m_bounce);
        QCOMPARE(renderer.m_bounceSettings.count, 20u);
        QVERIFY(renderer.m_primitiveCount > 0);
    }
    void sharedPhysicsAndTypedHistoryMatchFrozenCpp() {
        const QList<QList<QRect>> layouts={{{0,0,2560,1440},{2560,0,3440,1440},{6000,0,1920,1080}},
            {{-1600,-300,1600,900},{0,0,1920,1080},{2200,0,1280,1024}}};
        for(const auto &layout:layouts) for(int gravity:{-100,0,10,100}) for(int elasticity:{50,100}) for(bool collisions:{false,true}) {
            AnimationState state;FrozenAnimationState old;
            state.configureGeometries(layout,true,false,QStringLiteral("normal"),60,20,300,200,gravity,elasticity,collisions,true);
            old.configureGeometries(layout,true,false,QStringLiteral("normal"),60,20,300,200,gravity,elasticity,collisions,true);
            for(int frame=0;frame<180;++frame) {
                const qint64 now=1'000'000'000+qint64(frame)*16'666'667;
                state.advanceTo(now);old.advanceTo(now);
                // Includes 1 Hz delayed windows and mismatched output refresh.
                for(qint64 skew:{0LL,-8'333'333LL,-33'333'333LL,-999'000'000LL}) {
                    std::array<AnimationState::BallSnapshot,20> out;
                    const auto n=state.ballSnapshotAt(now+skew,out.data(),out.size());const auto expected=old.ballsAt(now+skew);
                    QCOMPARE(n,size_t(expected.size()));
                    for(size_t i=0;i<n;++i) {
                        const auto b=expected[int(i)].toMap();
                        QCOMPARE(out[i].x,b.value(QStringLiteral("x")).toDouble());QCOMPARE(out[i].y,b.value(QStringLiteral("y")).toDouble());
                        QCOMPARE(out[i].vx,b.value(QStringLiteral("vx")).toDouble());QCOMPARE(out[i].vy,b.value(QStringLiteral("vy")).toDouble());
                        QCOMPARE(out[i].size,b.value(QStringLiteral("size")).toDouble());
                    }
                }
            }
        }
    }
    void sharedWindowsSampleOwnPresentationWithoutDoubleStepping() {
        AnimationState state;
        state.configureGeometries({{0,0,1280,720}},true,false,QStringLiteral("normal"),60,20,300,200,10,100,true,true);
        VisualRenderer first,second;first.setSize({640,360});second.setSize({640,360});
        source(first,state);source(second,state);first.setViewport(state.bounds(),{});second.setViewport(state.bounds(),{});
        configure(first,1);configure(second,1);
        state.advanceTo(1'000'000'000);state.advanceTo(1'016'666'667);
        first.presentAt(1'008'333'333);const auto before=first.m_primitives;
        const auto current=state.ballsAt(1'016'666'667);
        state.advanceTo(1'033'333'333);second.presentAt(1'025'000'000);
        QCOMPARE(first.m_primitives.size(),before.size());
        QVERIFY(std::memcmp(first.m_primitives.data(),before.data(),before.size()*sizeof(visuals_core_primitive))==0);
        first.presentAt(1'008'333'333);
        QVERIFY(std::memcmp(first.m_primitives.data(),before.data(),before.size()*sizeof(visuals_core_primitive))==0);
        QCOMPARE(state.ballsAt(1'016'666'667),current);
        state.advanceTo(1'033'333'333);QCOMPARE(state.ballsAt(1'016'666'667),current);
    }
    void renderPaths_data() {
        QTest::addColumn<bool>("fallback");QTest::addColumn<int>("mode");
        for(int mode=0;mode<=2;++mode)for(bool fallback:{false,true})
            QTest::newRow(qPrintable(QString::number(mode)+(fallback?QStringLiteral("-plain"):QStringLiteral("-texture"))))<<fallback<<mode;
    }
    void renderPaths() {
        QFETCH(bool,fallback);QFETCH(int,mode);
        if(!gpu() && !fallback)QSKIP("OpenGL RHI proof runs under private KWin");
        QQuickWindow window;window.resize(640,360);window.setColor(Qt::black);
        window.setPersistentGraphics(false);window.setPersistentSceneGraph(false);
        AnimationState state;state.configureGeometries({{0,0,640,360}},true,false,QStringLiteral("normal"),60,20,300,200,10,100,true,true);
        VisualRenderer r(window.contentItem());r.setSize({640,360});source(r,state);r.setViewport(state.bounds(),{});
        if(fallback)r.m_supportCheck=[](QQuickWindow*){return false;};
        configure(r,mode);r.presentAt(1'000'000'000);r.presentBounceDelta(1.0/60.0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));const auto first=window.grabWindow();QVERIFY(!first.isNull());
        QCOMPARE(r.m_shaderInUse,gpu()&&!fallback);QCOMPARE(r.m_plainInUse,gpu()&&fallback);QVERIFY(r.m_primitiveCount>0);
        state.advance(1.0/60.0);r.presentAt(1'016'666'667);r.presentBounceDelta(1.0/60.0);
        const auto second=window.grabWindow();QVERIFY(first!=second);
        configure(r,mode,1,.25,.1,0,QStringLiteral("ember"),-100,50,false);
        const auto changed=window.grabWindow();QVERIFY(second!=changed);
        r.setSize({320,180});window.resize(320,180);
        QImage resized;
        QTRY_VERIFY((resized=window.grabWindow()).size()==QSize(qRound(320*window.effectiveDevicePixelRatio()),qRound(180*window.effectiveDevicePixelRatio())));
        if(!gpu())return;
        QSignalSpy invalidated(&window,&QQuickWindow::sceneGraphInvalidated);
        window.hide();window.releaseResources();QTRY_VERIFY(invalidated.count()>0);window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        QCOMPARE(window.grabWindow(),resized);
    }
    void textureMatchesPlain_data() {QTest::addColumn<int>("mode");for(int mode=0;mode<=2;++mode)QTest::newRow(qPrintable(QString::number(mode)))<<mode;}
    void textureMatchesPlain() {
        if(!gpu()) { QSKIP("OpenGL RHI required"); }
        QFETCH(int,mode);
        QQuickWindow window;window.resize(640,360);window.setColor(Qt::black);
        AnimationState state;state.configureGeometries({{0,0,1280,1440}},true,false,QStringLiteral("normal"),60,20,300,200,10,100,true,true);
        VisualRenderer texture(window.contentItem());texture.setSize({640,360});source(texture,state);texture.setViewport(state.bounds(),{});configure(texture,mode);
        texture.presentAt(1'000'000'000);texture.presentBounceDelta(1.0/60.0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));const auto a=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(texture.m_shaderInUse);
        texture.setVisible(false);VisualRenderer plain(window.contentItem());plain.setSize({640,360});source(plain,state);plain.setViewport(state.bounds(),{});
        plain.m_supportCheck=[](QQuickWindow*){return false;};configure(plain,mode);plain.presentAt(1'000'000'000);plain.presentBounceDelta(1.0/60.0);
        const auto b=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(plain.m_plainInUse);
        double sum=0;int maximum=0;for(int y=0;y<a.height();++y)for(int x=0;x<a.width()*4;++x){const int d=std::abs(a.constScanLine(y)[x]-b.constScanLine(y)[x]);sum+=d;maximum=std::max(d,maximum);}
        qInfo()<<"Texture/plain MAE"<<sum/(a.width()*a.height()*4)<<"max"<<maximum;
        QVERIFY(sum/(a.width()*a.height()*4)<.01);QVERIFY(maximum<=3);
    }
    void captures_data() {
        QTest::addColumn<int>("mode");QTest::addColumn<int>("scenario");
        for(int mode=0;mode<=2;++mode)for(int scenario=0;scenario<6;++scenario) {
            const QStringList modes={QStringLiteral("independent"),QStringLiteral("synchronized"),QStringLiteral("seamless")};
            const QStringList names={QStringLiteral("owner-ocean"),QStringLiteral("owner-ember"),QStringLiteral("maximum-ocean"),QStringLiteral("minimum-ember"),QStringLiteral("reduced-ember"),QStringLiteral("negative-gravity-ocean")};
            QTest::newRow(qPrintable(QStringLiteral("bounce-")+modes[mode]+QStringLiteral("-")+names[scenario]))<<mode<<scenario;
        }
    }
    void captures() {
        if(!gpu()) { QSKIP("Pixel parity requires private OpenGL RHI"); }
        QFETCH(int,mode);QFETCH(int,scenario);
        const int count=scenario==3?1:20;const double scale=scenario==3?.25:2, speed=scenario==3?.1:3, trails=scenario==3?0:1;
        const double gravity=scenario==2?100:scenario>=3?-100:10, elasticity=scenario>=2?50:100;
        const bool reduced=scenario==4,collisions=scenario!=3;
        const QString palette=(scenario==1||scenario==3||scenario==4)?QStringLiteral("ember"):QStringLiteral("ocean");
        const QSize size=scenario<2?QSize(3440,1440):scenario==3?QSize(1920,1080):QSize(2560,1440);
        QQuickWindow window;window.setFlags(Qt::FramelessWindowHint);window.resize(size);window.setColor(scenario==4?QColor(QStringLiteral("#18091d")):QColor(Qt::black));
        QQmlEngine engine;QQmlComponent cc(&engine);
        cc.setData("import QtQuick; QtObject { property int ballCount; property real animationDensity:55; property real animationScale; property real animationSpeed; property real trailAmount; property string animationPalette; property real ballGravity; property real ballElasticity; property bool ballCollisions; property string monitorBehavior; property var animationState; property var presentationClock:null; property real screenX:0; property real screenY:0 }",QUrl());
        std::unique_ptr<QObject> ctx(cc.create());QVERIFY(ctx);
        const QStringList modes={QStringLiteral("independent"),QStringLiteral("synchronized"),QStringLiteral("seamless")};
        ctx->setProperty("monitorBehavior",modes[mode]);ctx->setProperty("ballCount",count);ctx->setProperty("animationScale",scale*100);ctx->setProperty("animationSpeed",speed*100);
        ctx->setProperty("trailAmount",trails*100);ctx->setProperty("animationPalette",palette);ctx->setProperty("ballGravity",gravity);ctx->setProperty("ballElasticity",elasticity);ctx->setProperty("ballCollisions",collisions);
        AnimationState state;
        state.configureGeometries(mode==1 ? QList<QRect>{{0,0,2560,1440}} : QList<QRect>{{0,0,2560,1440},{2560,0,3440,1440},{6000,0,1920,1080}},!reduced,false,QStringLiteral("normal"),60,count,int(speed*100),int(scale*100),int(gravity),int(elasticity),collisions,true);
        ctx->setProperty("animationState",QVariant::fromValue(static_cast<QObject*>(&state)));
        QQmlComponent component(&engine,QUrl::fromLocalFile(QStringLiteral(BOUNCE_ORACLE_PATH)));QVERIFY2(component.isReady(),qPrintable(component.errorString()));
        std::unique_ptr<QObject> oracle(component.createWithInitialProperties({{QStringLiteral("context"),QVariant::fromValue(ctx.get())},{QStringLiteral("width"),size.width()},{QStringLiteral("height"),size.height()},{QStringLiteral("seed"),1},{QStringLiteral("reducedMotion"),reduced}}));
        auto *item=qobject_cast<QQuickItem*>(oracle.get());QVERIFY(item);item->setParentItem(window.contentItem());
        // No presentation clock: deterministic calls execute unchanged old physics.
        VisualRenderer native(window.contentItem());native.setSize(size);native.setVisible(false);source(native,state);native.setViewport(state.bounds(),{});
        configure(native,mode,count,scale,speed,trails,palette,gravity,elasticity,collisions,reduced);
        for(int i=0;i<(reduced?0:120);++i) {
            if(mode==0)QVERIFY(QMetaObject::invokeMethod(item,"stepPhysics",Q_ARG(QVariant,QVariant(1.0/60.0))));
            else state.advance(1.0/60.0);
            native.presentAt(1'000'000'000+qint64(i)*16'666'667);native.presentBounceDelta(1.0/60.0);
        }
        if(mode)QVERIFY(QMetaObject::invokeMethod(item,"sampleSharedBalls",Q_ARG(QVariant,QVariant(0))));
        QObject *canvas=nullptr;for(auto *child:item->findChildren<QObject*>())if(child->inherits("QQuickCanvasItem"))canvas=child;
        QVERIFY(canvas);QSignalSpy painted(canvas,SIGNAL(painted()));QVERIFY(QMetaObject::invokeMethod(canvas,"requestPaint"));
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));if(mode!=2)QTRY_VERIFY(painted.count()>0);
        const auto before=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!before.isNull());
        item->setVisible(false);native.setVisible(true);native.captureSnapshot(false);
        const auto after=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(native.m_shaderInUse);QCOMPARE(before.size(),after.size());
        double sum=0,squares=0;int maximum=0;quint64 different=0;QImage diff(before.size(),QImage::Format_RGBA8888);
        for(int y=0;y<before.height();++y)for(int x=0;x<before.width()*4;++x){const int d=std::abs(int(before.constScanLine(y)[x])-int(after.constScanLine(y)[x]));diff.scanLine(y)[x]=x%4==3?255:uchar(std::min(255,d*8));if(x%4!=3){sum+=d;squares+=d*d;maximum=std::max(maximum,d);different+=d!=0;}}
        const double channels=double(before.width())*before.height()*3;
        const QJsonObject metrics{{QStringLiteral("mean_absolute_rgb"),sum/channels},{QStringLiteral("rmse_rgb"),std::sqrt(squares/channels)},{QStringLiteral("max_rgb"),maximum},{QStringLiteral("different_rgb_fraction"),double(different)/channels},{QStringLiteral("width"),before.width()},{QStringLiteral("height"),before.height()},{QStringLiteral("records"),int(native.m_primitiveCount)}};
        qInfo().noquote()<<QJsonDocument(metrics).toJson(QJsonDocument::Compact);
        const auto dir=qEnvironmentVariable("VISUALS_CAPTURE_DIR");if(!dir.isEmpty()){
            const auto stem=dir+QStringLiteral("/")+QString::fromLatin1(QTest::currentDataTag());QVERIFY(before.save(stem+QStringLiteral("-old.png")));QVERIFY(after.save(stem+QStringLiteral("-new.png")));QVERIFY(diff.save(stem+QStringLiteral("-diff8x.png")));
            QImage pair(before.width()*2,before.height(),QImage::Format_RGB32);QPainter p(&pair);p.drawImage(0,0,before);p.drawImage(before.width(),0,after);p.end();QVERIFY(pair.save(stem+QStringLiteral("-side-by-side.png")));
            QFile file(stem+QStringLiteral("-metrics.json"));QVERIFY(file.open(QIODevice::WriteOnly));file.write(QJsonDocument(metrics).toJson());
        }
        QVERIFY2(sum/channels<1.0,"Bounce pixel MAE exceeded gradient/AA quantization allowance");
        QVERIFY2(std::sqrt(squares/channels)<3.0,"Bounce pixel RMSE exceeded gradient/AA allowance");
    }
};
QTEST_MAIN(BounceRendererTest)
#include "test_bouncerenderer.moc"
