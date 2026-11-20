// SPDX-License-Identifier: GPL-3.0-or-later
#include "visualrenderer.h"
#include "visualmaterial.h"
#include <QTest>
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QQmlEngine>
#include <QQmlComponent>
#include <QSignalSpy>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonDocument>
#include <QPainter>
#include <QDateTime>
#include <rhi/qrhi.h>
#include <cmath>

class DataProofItem final : public QQuickItem
{
public:
    explicit DataProofItem(QQuickItem *p) : QQuickItem(p) {setFlag(ItemHasContents);setSize({128,96});}
    float offset=4.25f;
    bool supported=false;
protected:
    QSGNode *updatePaintNode(QSGNode *old,UpdatePaintNodeData *) override {
        auto *node=static_cast<QSGGeometryNode *>(old);
        if (!node) {
            supported=VisualMaterial::supportsWindow(window(),VisualMaterial::VertexTextureProof);
            if (!supported) return nullptr;
            node=new QSGGeometryNode;
            auto *geometry=new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(),4);
            geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
            QSGGeometry::updateTexturedRectGeometry(geometry,QRectF(0,8,24,24),QRectF(0,0,1,1));
            node->setGeometry(geometry);node->setFlag(QSGNode::OwnsGeometry);
            node->setMaterial(new VisualMaterial(VisualMaterial::VertexTextureProof));node->setFlag(QSGNode::OwnsMaterial);
        }
        const float data[]={offset,0,0,0,.25f,.75f,.5f,1};
        auto *material=static_cast<VisualMaterial *>(node->material());
        material->dataTexture->setData(QByteArray(reinterpret_cast<const char *>(data),sizeof(data)),{2,1});
        node->markDirty(QSGNode::DirtyMaterial);
        return node;
    }
};

class VisualRendererTest final : public QObject
{
    Q_OBJECT
    bool gpu() const {
        if (qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU")) return true;
        return qEnvironmentVariable("QT_QUICK_BACKEND") != QStringLiteral("software")
            && QQuickWindow::graphicsApi()!=QSGRendererInterface::Software;
    }
    static void deterministic(VisualRenderer &renderer,int density,double scale,double speed,double trails,
        const QString &palette,int seed,bool reduced,double elapsed) {
        renderer.configureAurora(density,scale,speed,trails,palette,seed,reduced,QDateTime::currentMSecsSinceEpoch());
        renderer.m_config.epoch_ns=0;
        QCOMPARE(visuals_core_configure(renderer.m_core.get(),&renderer.m_config),0);
        renderer.presentAt(qint64(elapsed*1e9));
    }
private Q_SLOTS:
    void snapshotAndAbi() {
        QCOMPARE(visuals_core_abi_version(),VISUALS_CORE_ABI_VERSION);
        VisualRenderer renderer;renderer.setSize({3440,1440});
        deterministic(renderer,55,2,3,1,QStringLiteral("ocean"),1,false,12.5);
        QCOMPARE(renderer.m_snapshot.band_count,6u);
        QCOMPARE(renderer.m_snapshot.shade[3],.68f);
        const auto phase=renderer.m_snapshot.bands[0].geometry[1];
        renderer.setViewport(QRectF(-3440,-1080,7920,2520),QPointF(-3440,-1080));
        QCOMPARE(renderer.m_snapshot.bands[0].geometry[1],phase); // Mapping intentionally unchanged.
        deterministic(renderer,55,2,3,1,QStringLiteral("ocean"),1,true,200);
        QCOMPARE(renderer.m_snapshot.bands[0].geometry[1],phase);
        renderer.setSize({1720,720});
        QCOMPARE(renderer.m_snapshot.bands[0].geometry[1],phase*.5f);
    }
    void shaderPacks() {
        QVERIFY(VisualMaterial::shadersAvailable());
        QVERIFY(VisualMaterial::shadersAvailable(VisualMaterial::VertexTextureProof));
        for (const char *name : {"aurora.vert", "aurora.frag", "visual-data.vert"}) {
            QFile file(QStringLiteral(":/visuals/shaders/") + QString::fromLatin1(name) + QStringLiteral(".qsb"));
            QVERIFY(file.open(QIODevice::ReadOnly));
            const auto shader=QShader::fromSerialized(file.readAll());
            const auto blocks=shader.description().uniformBlocks();
            QCOMPARE(blocks.size(),1);
            QCOMPARE(blocks[0].size,VisualMaterial::UniformSize);
            QCOMPARE(blocks[0].members[2].offset,80);
            QCOMPARE(blocks[0].members[3].offset,512);
        }
        VisualMaterial material;
        QVERIFY(material.flags().testFlag(QSGMaterial::NoBatching));
        QVERIFY(material.flags().testFlag(QSGMaterial::RequiresFullMatrix));
    }
    void vertexDataTextureUpdatesAndRecreates() {
        if (!gpu()) QSKIP("Real RHI run is separate from software backend checks");
        QQuickWindow window;window.resize(128,96);window.setColor(Qt::black);
        window.setPersistentGraphics(false);window.setPersistentSceneGraph(false);
        DataProofItem proof(window.contentItem());
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        QCOMPARE(window.rendererInterface()->graphicsApi(),QSGRendererInterface::OpenGL);
        QImage first=window.grabWindow();QVERIFY(!first.isNull());QVERIFY(proof.supported);
        auto sample=[](const QImage &image,int x,int y) {return image.pixelColor(qRound(x*image.width()/128.0),qRound(y*image.height()/96.0));};
        QVERIFY(sample(first,12,16).green()>180);
        QVERIFY(sample(first,70,16).green()<5);
        proof.offset=60.75f;proof.update();
        QImage second=window.grabWindow();
        QVERIFY(sample(second,12,16).green()<5);
        QVERIFY(sample(second,70,16).green()>180);
        QSignalSpy invalidated(&window,&QQuickWindow::sceneGraphInvalidated);
        window.hide();window.releaseResources();QTRY_VERIFY(invalidated.count()>0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto recreated=window.grabWindow();QVERIFY(!recreated.isNull());
        QVERIFY(sample(recreated,70,16).green()>180);
        qInfo("RGBA32F nearest data texture: vertex-stage sampling, per-frame upload and resource recreation passed");
    }
    void rendererPathsAndResourceRecreation_data() {
        QTest::addColumn<bool>("fallback");
        QTest::newRow("shader")<<false;QTest::newRow("rejected-pipeline")<<true;
    }
    void rendererPathsAndResourceRecreation() {
        QFETCH(bool,fallback);
        if (!fallback && !gpu()) QSKIP("Real RHI run is separate from software backend checks");
        QQuickWindow window;window.resize(320,180);window.setColor(Qt::black);
        window.setPersistentGraphics(false);window.setPersistentSceneGraph(false);
        VisualRenderer renderer(window.contentItem());renderer.setSize({320,180});
        if (fallback) renderer.m_supportCheck=[](QQuickWindow *) {return false;};
        deterministic(renderer,55,2,3,1,QStringLiteral("ocean"),1,false,12.5);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto first=window.grabWindow();QVERIFY(!first.isNull());QCOMPARE(renderer.m_shaderInUse,!fallback);
        QVERIFY(first.pixelColor(first.width()/2,first.height()/2).blue()>50);
        deterministic(renderer,10,.25,.1,0,QStringLiteral("ember"),1,false,12.5);
        const auto changed=window.grabWindow();QVERIFY(first!=changed);
        // Offscreen software has no RHI/device to release. The real OpenGL
        // run below covers both shader and fallback resource recreation.
        if (window.rendererInterface()->graphicsApi()==QSGRendererInterface::Software) return;
        QSignalSpy invalidated(&window,&QQuickWindow::sceneGraphInvalidated);
        window.hide();window.releaseResources();QTRY_VERIFY(invalidated.count()>0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        const auto recreated=window.grabWindow();QVERIFY(!recreated.isNull());QCOMPARE(renderer.m_shaderInUse,!fallback);
        QCOMPARE(recreated,changed);
    }
    void oracleCaptures_data() {
        QTest::addColumn<int>("density");QTest::addColumn<double>("scale");QTest::addColumn<double>("speed");
        QTest::addColumn<double>("trails");QTest::addColumn<QString>("palette");QTest::addColumn<bool>("reduced");
        QTest::addColumn<QSize>("size");QTest::addColumn<QString>("background");
        QTest::newRow("owner-ocean")<<55<<2.0<<3.0<<1.0<<QStringLiteral("ocean")<<false<<QSize(3440,1440)<<QStringLiteral("black");
        QTest::newRow("minimum-ember")<<10<<.25<<.1<<0.0<<QStringLiteral("ember")<<false<<QSize(1920,1080)<<QStringLiteral("black");
        QTest::newRow("maximum-spectrum")<<100<<2.0<<3.0<<1.0<<QStringLiteral("spectrum")<<false<<QSize(2560,1440)<<QStringLiteral("black");
        QTest::newRow("reduced-forest")<<55<<2.0<<3.0<<1.0<<QStringLiteral("forest")<<true<<QSize(1280,720)<<QStringLiteral("#18091d");
        QTest::newRow("default-mono")<<50<<1.0<<1.0<<.35<<QStringLiteral("mono")<<false<<QSize(1280,720)<<QStringLiteral("#062f3c");
        QTest::newRow("default-pastel")<<50<<1.0<<1.0<<.35<<QStringLiteral("pastel")<<false<<QSize(1280,720)<<QStringLiteral("black");
    }
    void oracleCaptures() {
        if (!gpu()) QSKIP("Pixel parity requires real OpenGL RHI");
        QFETCH(int,density);QFETCH(double,scale);QFETCH(double,speed);QFETCH(double,trails);QFETCH(QString,palette);
        QFETCH(bool,reduced);QFETCH(QSize,size);QFETCH(QString,background);
        QQuickWindow window;window.setFlags(Qt::FramelessWindowHint);window.resize(size);window.setColor(QColor(background));
        QQmlEngine engine;
        QObject context;
        context.setProperty("animationDensity",density);context.setProperty("animationScale",scale*100);
        context.setProperty("animationSpeed",speed*100);context.setProperty("trailAmount",trails*100);
        context.setProperty("animationPalette",palette);
        // Dynamic QObject properties are not QML properties; build a typed oracle context.
        QQmlComponent cc(&engine);
        cc.setData("import QtQuick; QtObject { property real animationDensity; property real animationScale; property real animationSpeed; property real trailAmount; property string animationPalette; property var presentationClock: null }",QUrl());
        std::unique_ptr<QObject> ctx(cc.create());QVERIFY(ctx);
        for (const auto *name : {"animationDensity","animationScale","animationSpeed","trailAmount","animationPalette"}) ctx->setProperty(name,context.property(name));
        QQmlComponent component(&engine,QUrl::fromLocalFile(QStringLiteral(AURORA_ORACLE_PATH)));
        QVERIFY2(component.isReady(),qPrintable(component.errorString()));
        std::unique_ptr<QObject> oracle(component.createWithInitialProperties({{QStringLiteral("context"),QVariant::fromValue(ctx.get())},
            {QStringLiteral("width"),size.width()},{QStringLiteral("height"),size.height()},{QStringLiteral("seed"),1},{QStringLiteral("reducedMotion"),reduced},
            {QStringLiteral("phase"),1.0/159.0+(reduced?0:12.5*.54*speed)}}));
        auto *item=qobject_cast<QQuickItem *>(oracle.get());QVERIFY(item);item->setParentItem(window.contentItem());
        QSignalSpy painted(item,SIGNAL(painted()));
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));QTRY_VERIFY(painted.count()>0);
        QCOMPARE(window.size(),size);
        QImage before=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!before.isNull());
        QCOMPARE(before.size(),QSize(qRound(size.width()*window.effectiveDevicePixelRatio()),
                                    qRound(size.height()*window.effectiveDevicePixelRatio())));
        item->setVisible(false);
        VisualRenderer renderer(window.contentItem());renderer.setSize(size);
        deterministic(renderer,density,scale,speed,trails,palette,1,reduced,12.5);
        QImage after=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!after.isNull());
        QVERIFY(renderer.m_shaderInUse);QCOMPARE(before.size(),after.size());
        double sum=0,squares=0;int maximum=0;quint64 different=0;
        QImage difference(before.size(),QImage::Format_RGBA8888);
        for (int y=0;y<before.height();++y) for (int x=0;x<before.width()*4;++x) {
            const int d=std::abs(int(before.constScanLine(y)[x])-int(after.constScanLine(y)[x]));
            difference.scanLine(y)[x]=x%4==3?255:uchar(qMin(255,d*8));
            if (x%4!=3) {sum+=d;squares+=d*d;maximum=qMax(maximum,d);different+=d!=0;}
        }
        const double channels=double(before.width())*before.height()*3;
        const QJsonObject metrics{{QStringLiteral("mean_absolute_rgb"),sum/channels},{QStringLiteral("rmse_rgb"),std::sqrt(squares/channels)},
            {QStringLiteral("max_rgb"),maximum},{QStringLiteral("different_rgb_fraction"),double(different)/channels},
            {QStringLiteral("width"),before.width()},{QStringLiteral("height"),before.height()},{QStringLiteral("qt"),QString::fromLatin1(qVersion())}};
        qInfo().noquote()<<QJsonDocument(metrics).toJson(QJsonDocument::Compact);
        const auto directory=qEnvironmentVariable("VISUALS_CAPTURE_DIR");
        if (!directory.isEmpty()) {
            QDir().mkpath(directory);const QString stem=directory+QStringLiteral("/")+QString::fromLatin1(QTest::currentDataTag());
            QVERIFY(before.save(stem+QStringLiteral("-old.png")));QVERIFY(after.save(stem+QStringLiteral("-new.png")));QVERIFY(difference.save(stem+QStringLiteral("-diff8x.png")));
            QImage pair(before.width()*2,before.height(),QImage::Format_RGB32);QPainter painter(&pair);
            painter.drawImage(0,0,before);painter.drawImage(before.width(),0,after);painter.end();QVERIFY(pair.save(stem+QStringLiteral("-side-by-side.png")));
            QFile file(stem+QStringLiteral("-metrics.json"));QVERIFY(file.open(QIODevice::WriteOnly));file.write(QJsonDocument(metrics).toJson());
        }
        QVERIFY2(sum/channels<2.0,"Mean pixel error exceeded documented QPainter gradient quantization tolerance");
        QVERIFY2(maximum<20,"Large pixel error suggests gradient/compositing mismatch");
    }
};
QTEST_MAIN(VisualRendererTest)
#include "test_visualrenderer.moc"
