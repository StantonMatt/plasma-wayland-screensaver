// SPDX-License-Identifier: GPL-3.0-or-later
#include "strokerenderer.h"
#include "strokematerial.h"
#include <QTest>
#include <QQuickWindow>
#include <QQmlEngine>
#include <QQmlComponent>
#include <QSignalSpy>
#include <QDir>
#include <QFile>
#include <QJsonObject>
#include <QJsonDocument>
#include <QPainter>
#include <QPainterPath>
#include <QDateTime>
#include <rhi/qrhi.h>
#include <cmath>
#include <cstring>
#include <array>
#include <algorithm>

class StrokeRendererTest final : public QObject
{
    Q_OBJECT
    bool gpu() const {
        return qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU") ||
            (qEnvironmentVariable("QT_QUICK_BACKEND")!=QStringLiteral("software") && QQuickWindow::graphicsApi()!=QSGRendererInterface::Software);
    }
    static void deterministic(StrokeRenderer &renderer,const QString &visual,int density,double scale,double speed,
        double trails,const QString &palette,int seed,bool reduced,double elapsed) {
        renderer.configureStrokes(visual,density,scale,speed,trails,palette,seed,reduced,QDateTime::currentMSecsSinceEpoch());
        renderer.m_config.epoch_ns=0;
        QCOMPARE(visuals_strokes_configure(renderer.m_core.get(),&renderer.m_config),0);
        renderer.presentAt(qint64(elapsed*1e9));
    }
    static std::unique_ptr<QObject> root(QQmlEngine &engine,QQuickWindow &window,const QString &visual) {
        QQmlComponent component(&engine,QUrl::fromLocalFile(QStringLiteral(STROKES_SOURCE_PATH)+QStringLiteral("/")+visual+QStringLiteral(".qml")));
        if (!component.isReady()) {qWarning()<<component.errorString();return {};}
        std::unique_ptr<QObject> result(component.createWithInitialProperties({{QStringLiteral("width"),window.width()},
            {QStringLiteral("height"),window.height()},{QStringLiteral("reducedMotion"),true}}));
        auto *item=qobject_cast<QQuickItem *>(result.get());if (item) item->setParentItem(window.contentItem());return result;
    }
private Q_SLOTS:
    void snapshotAndAbi() {
        QCOMPARE(visuals_strokes_abi_version(),VISUALS_STROKES_ABI_VERSION);
        for (const auto &visual:{QStringLiteral("ribbons"),QStringLiteral("kaleidoscope")}) {
            StrokeRenderer renderer;renderer.setSize({3440,1440});
            deterministic(renderer,visual,55,2,3,1,QStringLiteral("ocean"),1,false,12.5);
            QVERIFY(renderer.m_vertices.size()>1000);const auto vertices=renderer.m_vertices;
            renderer.setViewport(QRectF(-3440,-1080,7920,2520),QPointF(-3440,-1080));
            QCOMPARE(renderer.m_vertices.size(),vertices.size());
            QCOMPARE(std::memcmp(renderer.m_vertices.data(),vertices.data(),vertices.size()*sizeof(visuals_strokes_vertex)),0);
            deterministic(renderer,visual,55,2,3,1,QStringLiteral("ocean"),1,true,200);
            QCOMPARE(std::memcmp(renderer.m_vertices.data(),vertices.data(),vertices.size()*sizeof(visuals_strokes_vertex)),0);
            const auto *storage=renderer.m_vertices.data();
            deterministic(renderer,visual,55,2,3,1,QStringLiteral("ocean"),1,false,22.5);
            QCOMPARE(renderer.m_vertices.data(),storage);
            QVERIFY(std::memcmp(renderer.m_vertices.data(),vertices.data(),vertices.size()*sizeof(visuals_strokes_vertex))!=0);
        }
    }
    void shaderPacks() {
        QVERIFY(StrokeMaterial::shadersAvailable());
        for (const auto *name:{"strokes.vert","strokes.frag"}) {
            QFile file(QStringLiteral(":/visuals/shaders/")+QString::fromLatin1(name)+QStringLiteral(".qsb"));QVERIFY(file.open(QIODevice::ReadOnly));
            const auto shader=QShader::fromSerialized(file.readAll());QVERIFY(shader.isValid());
            const auto blocks=shader.description().uniformBlocks();QCOMPARE(blocks.size(),1);QCOMPARE(blocks[0].size,StrokeMaterial::UniformSize);
            QCOMPARE(blocks[0].members[1].offset,64);
        }
        StrokeMaterial material;QVERIFY(material.flags().testFlag(QSGMaterial::NoBatching));QVERIFY(material.flags().testFlag(QSGMaterial::RequiresFullMatrix));
    }
    void additiveLayerClampsAlphaAndCirclePairsUseUnionCoverage() {
        if (!gpu()) QSKIP("Layer blend proof requires real OpenGL RHI");
        QQuickWindow window;window.resize(64,64);window.setColor(QColor(40,80,120));
        QQmlEngine engine;auto component=root(engine,window,QStringLiteral("Ribbons"));QVERIFY(component);
        auto *item=qobject_cast<QQuickItem *>(component.get());QVERIFY(item);
        StrokeRenderer renderer(item);renderer.setSize({64,64});
        component->setProperty("nativeRenderer",QVariant::fromValue(static_cast<QObject *>(&renderer)));
        renderer.m_vertices.clear();
        auto pair=[&](float x1,float x2,float radius,const std::array<float,4> &color) {
            const float left=qMin(x1,x2)-radius-1,right=qMax(x1,x2)+radius+1;
            const float top=32-radius-1,bottom=32+radius+1;
            const std::array<std::array<float,2>,4> corners={{{left,top},{left,bottom},{right,top},{right,bottom}}};
            std::array<visuals_strokes_vertex,4> v{};
            for (int i=0;i<4;++i) {
                v[i].position[0]=corners[i][0];v[i].position[1]=corners[i][1];
                v[i].shape[0]=corners[i][0]-x1;v[i].shape[1]=corners[i][1]-32;
                v[i].shape[2]=corners[i][0]-x2;v[i].shape[3]=corners[i][1]-32;
                v[i].parameters[0]=v[i].parameters[1]=radius;
                std::copy(color.begin(),color.end(),v[i].color);
            }
            for (int index:{0,1,2,2,1,3}) renderer.m_vertices.push_back(v[index]);
        };
        pair(32,40,10,{1,0,0,.6f});pair(32,32,3,{0,0,1,.6f});renderer.update();
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto capture=window.grabWindow();QVERIFY(renderer.m_shaderInUse);
        auto sample=[&](int x,int y) {return capture.pixelColor(qRound(x*window.effectiveDevicePixelRatio()),qRound(y*window.effectiveDevicePixelRatio()));};
        auto close=[](QColor actual,QColor expected) {
            return qAbs(actual.red()-expected.red())<=2 && qAbs(actual.green()-expected.green())<=2 && qAbs(actual.blue()-expected.blue())<=2;
        };
        // Two 0.6-alpha primitives saturate the RGBA8 layer's alpha at one,
        // before source-over. The background must not leak into this pixel.
        QVERIFY2(close(sample(32,32),QColor(153,0,153)),qPrintable(sample(32,32).name()));
        // This point is inside both red circles, but the pair is filled once.
        QVERIFY2(close(sample(38,32),QColor(169,32,48)),qPrintable(sample(38,32).name()));
        QCOMPARE(sample(5,5),window.color());
    }
    void singleStrokeCoverageAtFoldedJoin() {
        if (!gpu()) QSKIP("Coverage union proof requires real OpenGL RHI");
        QQuickWindow window;auto format=window.format();format.setAlphaBufferSize(8);window.setFormat(format);
        window.resize(3440,1440);window.setColor(Qt::transparent);
        QQmlEngine engine;auto component=root(engine,window,QStringLiteral("Ribbons"));QVERIFY(component);
        auto *item=qobject_cast<QQuickItem *>(component.get());QVERIFY(item);
        StrokeRenderer renderer(item);renderer.setSize({3440,1440});
        component->setProperty("nativeRenderer",QVariant::fromValue(static_cast<QObject *>(&renderer)));
        deterministic(renderer,QStringLiteral("ribbons"),55,2,3,1,QStringLiteral("ocean"),1,false,5.52/.55);
        // Eight ribbons, main then glow. Isolate ribbon 2's 56px glow stroke.
        const size_t count=renderer.m_vertices.size()/16;
        renderer.m_vertices=std::vector<visuals_strokes_vertex>(renderer.m_vertices.begin()+5*count,renderer.m_vertices.begin()+6*count);
        renderer.update();window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto after=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(renderer.m_shaderInUse);
        // Controls from the frozen oracle/Ribbons.qml, at the reviewer's exact
        // seed/phase/settings trigger. QPainter fills the complete stroke once.
        const double w=3440,h=1440,y=h*(.12+2.0/7*.76),wave=5.52*3+2*.71+.01;
        QPainterPath path;path.moveTo(-w*.05,y+std::sin(wave)*h*.08);
        path.cubicTo(w*.22,y+std::cos(wave*1.2)*h*.28,w*.38,y+std::sin(wave*.83+2)*h*.25,w*.55,y+std::cos(wave+1)*h*.12);
        path.cubicTo(w*.72,y+std::sin(wave*1.31)*h*.27,w*.88,y+std::cos(wave*.77+3)*h*.24,w*1.05,y+std::sin(wave+2)*h*.08);
        QImage before(after.size(),QImage::Format_RGBA8888_Premultiplied);before.fill(Qt::transparent);
        QPainter painter(&before);painter.setRenderHint(QPainter::Antialiasing);painter.scale(window.effectiveDevicePixelRatio(),window.effectiveDevicePixelRatio());
        const auto &v=renderer.m_vertices.front();QPen pen(QColor::fromRgbF(v.color[0],v.color[1],v.color[2],.08),56,Qt::SolidLine,Qt::FlatCap,Qt::MiterJoin);
        painter.setPen(pen);painter.drawPath(path);painter.end();
        int oldMax=0,newMax=0,over=0;quint64 oldEnergy=0,newEnergy=0;
        for (int y=0;y<after.height();++y) for (int x=0;x<after.width();++x) {
            const int a=before.pixelColor(x,y).alpha(),b=after.pixelColor(x,y).alpha();
            oldMax=qMax(oldMax,a);newMax=qMax(newMax,b);over+=b>20;oldEnergy+=a;newEnergy+=b;
        }
        const QJsonObject metrics{{QStringLiteral("oracle_max_alpha"),oldMax},{QStringLiteral("native_max_alpha"),newMax},{QStringLiteral("pixels_above20"),over},
            {QStringLiteral("oracle_alpha_sum"),double(oldEnergy)},{QStringLiteral("native_alpha_sum"),double(newEnergy)}};
        qInfo().noquote()<<"single-stroke-trigger"<<QJsonDocument(metrics).toJson(QJsonDocument::Compact);
        const auto directory=qEnvironmentVariable("VISUALS_CAPTURE_DIR");
        if (!directory.isEmpty()) {
            QVERIFY(before.save(directory+QStringLiteral("/single-stroke-trigger-old.png")));QVERIFY(after.save(directory+QStringLiteral("/single-stroke-trigger-new.png")));
            QFile file(directory+QStringLiteral("/single-stroke-trigger-metrics.json"));QVERIFY(file.open(QIODevice::WriteOnly));file.write(QJsonDocument(metrics).toJson());
        }
        QCOMPARE(oldMax,20);QVERIFY2(newMax==20,"A stroke added its own alpha repeatedly at a folded join");QCOMPARE(over,0);
        QVERIFY(newEnergy>oldEnergy*.95 && newEnergy<oldEnergy*1.05);
    }
    void rendererPathsAndResourceRecreation_data() {
        QTest::addColumn<QString>("visual");QTest::addColumn<bool>("fallback");
        for (const auto *visual:{"Ribbons","Kaleidoscope"}) for (bool fallback:{false,true})
            QTest::newRow(qPrintable(QString::fromLatin1(visual)+(fallback?QStringLiteral("-rejected-pipeline"):QStringLiteral("-shader"))))<<QString::fromLatin1(visual)<<fallback;
    }
    void rendererPathsAndResourceRecreation() {
        QFETCH(QString,visual);QFETCH(bool,fallback);
        if (!fallback && !gpu()) QSKIP("Real RHI run is separate from software backend checks");
        QQuickWindow window;window.resize(320,180);window.setColor(QColor(QStringLiteral("#062f3c")));
        window.setPersistentGraphics(false);window.setPersistentSceneGraph(false);
        QQmlEngine engine;auto component=root(engine,window,visual);QVERIFY(component);
        auto *item=qobject_cast<QQuickItem *>(component.get());QVERIFY(item);
        StrokeRenderer renderer(item);renderer.setSize(item->size());
        if (fallback) renderer.m_supportCheck=[](QQuickWindow *) {return false;};
        component->setProperty("nativeRenderer",QVariant::fromValue(static_cast<QObject *>(&renderer)));
        deterministic(renderer,visual.toLower(),55,2,3,1,QStringLiteral("ocean"),1,false,12.5);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto first=window.grabWindow();QVERIFY(!first.isNull());QCOMPARE(renderer.m_shaderInUse,!fallback);
        int changedPixels=0;for (int y=0;y<first.height();++y) for (int x=0;x<first.width();++x) changedPixels+=first.pixelColor(x,y)!=window.color();
        QVERIFY(changedPixels>100);
        deterministic(renderer,visual.toLower(),10,.25,.1,0,QStringLiteral("ember"),1,false,12.5);
        const auto changed=window.grabWindow();QVERIFY(first!=changed);
        deterministic(renderer,visual.toLower(),10,.25,.1,0,QStringLiteral("ember"),1,false,22.5);
        const auto animated=window.grabWindow();QVERIFY(animated!=changed);
        if (window.rendererInterface()->graphicsApi()==QSGRendererInterface::Software) return;
        QSignalSpy invalidated(&window,&QQuickWindow::sceneGraphInvalidated);
        window.hide();window.releaseResources();QTRY_VERIFY(invalidated.count()>0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto recreated=window.grabWindow();QVERIFY(!recreated.isNull());QCOMPARE(renderer.m_shaderInUse,!fallback);QCOMPARE(recreated,animated);
    }
    void oracleCaptures_data() {
        QTest::addColumn<QString>("visual");QTest::addColumn<int>("density");QTest::addColumn<double>("scale");QTest::addColumn<double>("speed");
        QTest::addColumn<double>("trails");QTest::addColumn<QString>("palette");QTest::addColumn<bool>("reduced");
        QTest::addColumn<QSize>("size");QTest::addColumn<QString>("background");
        for (const auto *name:{"Ribbons","Kaleidoscope"}) {
            const auto v=QString::fromLatin1(name);
            auto row=[&](const char *scenario)->QTestData& {return QTest::newRow(qPrintable(v+QStringLiteral("-")+QString::fromLatin1(scenario)))<<v;};
            row("owner-ocean")<<55<<2.0<<3.0<<1.0<<QStringLiteral("ocean")<<false<<QSize(3440,1440)<<QStringLiteral("black");
            row("owner-coloured")<<55<<2.0<<3.0<<1.0<<QStringLiteral("ocean")<<false<<QSize(1280,720)<<QStringLiteral("#39264c");
            row("minimum-ember")<<10<<.25<<.1<<0.0<<QStringLiteral("ember")<<false<<QSize(1920,1080)<<QStringLiteral("black");
            row("maximum-spectrum")<<100<<2.0<<3.0<<1.0<<QStringLiteral("spectrum")<<false<<QSize(2560,1440)<<QStringLiteral("black");
            row("reduced-forest")<<55<<2.0<<3.0<<1.0<<QStringLiteral("forest")<<true<<QSize(1280,720)<<QStringLiteral("#18091d");
            row("default-mono")<<50<<1.0<<1.0<<.35<<QStringLiteral("mono")<<false<<QSize(1280,720)<<QStringLiteral("#062f3c");
            row("default-pastel")<<50<<1.0<<1.0<<.35<<QStringLiteral("pastel")<<false<<QSize(1280,720)<<QStringLiteral("black");
            row("glow-threshold")<<55<<2.0<<3.0<<.25<<QStringLiteral("ocean")<<false<<QSize(1280,720)<<QStringLiteral("#39264c");
        }
    }
    void oracleCaptures() {
        if (!gpu()) QSKIP("Pixel parity requires real OpenGL RHI");
        QFETCH(QString,visual);QFETCH(int,density);QFETCH(double,scale);QFETCH(double,speed);QFETCH(double,trails);QFETCH(QString,palette);
        QFETCH(bool,reduced);QFETCH(QSize,size);QFETCH(QString,background);
        QQuickWindow window;window.setFlags(Qt::FramelessWindowHint);window.resize(size);window.setColor(QColor(background));
        QQmlEngine engine;QQmlComponent cc(&engine);
        cc.setData("import QtQuick; QtObject { property real animationDensity; property real animationScale; property real animationSpeed; property real trailAmount; property string animationPalette; property var presentationClock: null }",QUrl());
        std::unique_ptr<QObject> ctx(cc.create());QVERIFY(ctx);
        ctx->setProperty("animationDensity",density);ctx->setProperty("animationScale",scale*100);ctx->setProperty("animationSpeed",speed*100);
        ctx->setProperty("trailAmount",trails*100);ctx->setProperty("animationPalette",palette);
        QQmlComponent oc(&engine,QUrl::fromLocalFile(QStringLiteral(STROKES_ORACLE_PATH)+QStringLiteral("/")+visual+QStringLiteral(".qml")));
        QVERIFY2(oc.isReady(),qPrintable(oc.errorString()));
        const double phase=reduced?0:12.5*(visual==QStringLiteral("Ribbons")?.55:1.0);
        std::unique_ptr<QObject> oracle(oc.createWithInitialProperties({{QStringLiteral("context"),QVariant::fromValue(ctx.get())},
            {QStringLiteral("width"),size.width()},{QStringLiteral("height"),size.height()},{QStringLiteral("seed"),1},
            {QStringLiteral("reducedMotion"),reduced},{QStringLiteral("phase"),phase}}));
        auto *oldItem=qobject_cast<QQuickItem *>(oracle.get());QVERIFY(oldItem);oldItem->setParentItem(window.contentItem());
        QSignalSpy painted(oldItem,SIGNAL(painted()));window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));QTRY_VERIFY(painted.count()>0);
        QCOMPARE(window.rendererInterface()->graphicsApi(),QSGRendererInterface::OpenGL);
        const auto before=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!before.isNull());
        QCOMPARE(before.size(),QSize(qRound(size.width()*window.effectiveDevicePixelRatio()),qRound(size.height()*window.effectiveDevicePixelRatio())));
        oldItem->setVisible(false);
        auto component=root(engine,window,visual);QVERIFY(component);auto *item=qobject_cast<QQuickItem *>(component.get());QVERIFY(item);
        StrokeRenderer renderer(item);renderer.setSize(size);
        component->setProperty("nativeRenderer",QVariant::fromValue(static_cast<QObject *>(&renderer)));
        deterministic(renderer,visual.toLower(),density,scale,speed,trails,palette,1,reduced,12.5);
        const auto after=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!after.isNull());QVERIFY(renderer.m_shaderInUse);QCOMPARE(before.size(),after.size());
        double sum=0,squares=0,oldEnergy=0,newEnergy=0;int maximum=0;quint64 different=0,edgeDifferences=0;
        QImage difference(before.size(),QImage::Format_RGBA8888);
        for (int y=0;y<before.height();++y) for (int x=0;x<before.width()*4;++x) {
            const int a=before.constScanLine(y)[x],b=after.constScanLine(y)[x];const int d=std::abs(a-b);
            difference.scanLine(y)[x]=x%4==3?255:uchar(qMin(255,d*8));
            if (x%4!=3) {sum+=d;squares+=d*d;maximum=qMax(maximum,d);different+=d!=0;edgeDifferences+=d>20;oldEnergy+=a;newEnergy+=b;}
        }
        const double channels=double(before.width())*before.height()*3;
        const QJsonObject metrics{{QStringLiteral("mean_absolute_rgb"),sum/channels},{QStringLiteral("rmse_rgb"),std::sqrt(squares/channels)},
            {QStringLiteral("max_rgb"),maximum},{QStringLiteral("different_rgb_fraction"),double(different)/channels},
            {QStringLiteral("over20_rgb_fraction"),double(edgeDifferences)/channels},{QStringLiteral("old_rgb_sum"),oldEnergy},
            {QStringLiteral("new_rgb_sum"),newEnergy},{QStringLiteral("vertices"),int(renderer.m_vertices.size())},
            {QStringLiteral("width"),before.width()},{QStringLiteral("height"),before.height()},{QStringLiteral("qt"),QString::fromLatin1(qVersion())}};
        qInfo().noquote()<<QTest::currentDataTag()<<QJsonDocument(metrics).toJson(QJsonDocument::Compact);
        const auto directory=qEnvironmentVariable("VISUALS_CAPTURE_DIR");
        if (!directory.isEmpty()) {
            QDir().mkpath(directory);const auto stem=directory+QStringLiteral("/")+QString::fromLatin1(QTest::currentDataTag());
            QVERIFY(before.save(stem+QStringLiteral("-old.png")));QVERIFY(after.save(stem+QStringLiteral("-new.png")));QVERIFY(difference.save(stem+QStringLiteral("-diff8x.png")));
            QImage pair(before.width()*2,before.height(),QImage::Format_RGB32);pair.fill(Qt::black);
            QPainter painter(&pair);
            // Captures retain their window DPR. Explicit physical target rects
            // prevent QPainter from shrinking them a second time in this export.
            painter.drawImage(QRect(0,0,before.width(),before.height()),before);
            painter.drawImage(QRect(before.width(),0,after.width(),after.height()),after);
            painter.end();QVERIFY(pair.save(stem+QStringLiteral("-side-by-side.png")));
            QFile file(stem+QStringLiteral("-metrics.json"));QVERIFY(file.open(QIODevice::WriteOnly));file.write(QJsonDocument(metrics).toJson());
        }
        if (QColor(background)==QColor(Qt::black) && oldEnergy>0)
            QVERIFY2(std::abs(newEnergy-oldEnergy)/oldEnergy<.15,"Sparse strokes/discs lost too much total RGB energy");
        QVERIFY2(sum/channels<1.0,"Mean pixel error exceeds the allowed edge AA tolerance");
        QVERIFY2(std::sqrt(squares/channels)<3.0,"RGB RMSE exceeds the edge AA tolerance");
        QVERIFY2(double(edgeDifferences)/channels<.003,"Widespread pixel differences suggest topology or layer blend mismatch");
    }
};
QTEST_MAIN(StrokeRendererTest)
#include "test_strokerenderer.moc"
