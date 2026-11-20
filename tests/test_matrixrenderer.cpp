// SPDX-License-Identifier: GPL-3.0-or-later
#include "matrixrenderer.h"
#include <QTest>
#include <QQuickWindow>
#include <QQmlEngine>
#include <QQmlComponent>
#include <QSignalSpy>
#include <QDir>
#include <QFile>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonDocument>
#include <QPainter>
#include <QDateTime>
#include <cmath>
class MatrixRendererTest final:public QObject {
    Q_OBJECT
    bool gpu()const{return qEnvironmentVariableIsSet("VISUALS_REQUIRE_GPU")||qEnvironmentVariable("QT_QUICK_BACKEND")!=QStringLiteral("software");}
    static void deterministic(MatrixRenderer &r,int density=55,double scale=2,double speed=3,double trails=1,const QString &palette=QStringLiteral("ocean"),int seed=1,bool reduced=false,double elapsed=12.5){
        r.configureMatrix(density,scale,speed,trails,palette,seed,reduced,QDateTime::currentMSecsSinceEpoch());r.m_config.epoch_ns=0;
        QCOMPARE(visuals_matrix_configure(r.m_core.get(),&r.m_config),0);r.presentAt(qint64(elapsed*1e9));
    }
private Q_SLOTS:
    void abiAndStaticLayout(){
        QCOMPARE(visuals_matrix_abi_version(),VISUALS_MATRIX_ABI_VERSION);QVERIFY(MatrixMaterial::shadersAvailable());QVERIFY(MatrixMaterial::shadersAvailable(true));
        MatrixRenderer r;r.setSize({3440,1440});deterministic(r);QCOMPARE(r.m_layout.glyph_size,47u);QCOMPARE(r.m_layout.column_count,64u);QVERIFY(!r.m_atlas.isNull());QCOMPARE(r.m_atlas.format(),QImage::Format_Alpha8);QCOMPARE(r.m_atlas.sizeInBytes(),qsizetype(r.m_atlas.width())*r.m_atlas.height());
        const auto revision=r.m_revision;const auto cacheKey=r.m_atlas.cacheKey();const auto vertices=r.m_vertices.data();const auto first=r.m_offsets;
        r.presentAt(13'000'000'000);QVERIFY(r.m_offsets!=first);QCOMPARE(r.m_revision,revision);QCOMPARE(r.m_atlas.cacheKey(),cacheKey);QCOMPARE(r.m_vertices.data(),vertices);
        r.setSize({1920,1080});QVERIFY(r.m_revision>revision);QCOMPARE(r.m_atlas.cacheKey(),cacheKey);
        deterministic(r,10,.25,.1,0,QStringLiteral("ember"),-1197,true,200);QCOMPARE(r.m_layout.glyph_size,11u);QVERIFY(r.m_atlas.cacheKey()!=cacheKey);
    }
    void pathsAndResourceRecreation_data(){QTest::addColumn<int>("path");QTest::newRow("texture")<<0;QTest::newRow("rejected-texture")<<1;QTest::newRow("rejected-all")<<2;}
    void pathsAndResourceRecreation(){
        QFETCH(int,path);if(!gpu()&&path!=2)QSKIP("Real OpenGL proof is separate");
        QQuickWindow window;window.resize(320,180);window.setColor(Qt::black);window.setPersistentGraphics(false);window.setPersistentSceneGraph(false);
        MatrixRenderer r(window.contentItem());r.setSize({320,180});
        if(path==1)r.m_supportCheck=[](QQuickWindow *w,int n,bool plain){return plain&&MatrixMaterial::supportsWindow(w,n,true);};
        if(path==2)r.m_supportCheck=[](QQuickWindow *,int,bool){return false;};
        deterministic(r);window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));const auto first=window.grabWindow();QVERIFY(!first.isNull());
        QCOMPARE(r.m_shaderInUse,path==0);QCOMPARE(r.m_plainInUse,path==1);bool ink=false;for(int y=0;y<first.height();++y)for(int x=0;x<first.width();++x)ink|=first.pixelColor(x,y)!=QColor(Qt::black);QVERIFY(ink);
        r.presentAt(13'000'000'000);const auto second=window.grabWindow();QVERIFY(second!=first);
        deterministic(r,10,.25,.1,0,QStringLiteral("ember"),-1197);const auto changed=window.grabWindow();QVERIFY(changed!=second);
        if(!gpu())return;
        QSignalSpy invalidated(&window,&QQuickWindow::sceneGraphInvalidated);window.hide();window.releaseResources();QTRY_VERIFY(invalidated.count()>0);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));const auto recreated=window.grabWindow();QCOMPARE(recreated,changed);
        r.setSize({640,360});window.resize(640,360);deterministic(r,100,2,3,1,QStringLiteral("pastel"),429);QVERIFY(window.grabWindow()!=recreated);
    }
    void resourceDimensionsAreReprobed(){
        if(!gpu())QSKIP("Real OpenGL required");
        QQuickWindow window;window.resize(320,180);window.setColor(Qt::black);MatrixRenderer r(window.contentItem());r.setSize({320,180});
        r.m_supportCheck=[](QQuickWindow *w,int columns,bool plain){return !plain&&columns<20&&MatrixMaterial::supportsWindow(w,columns);};
        deterministic(r);window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));QVERIFY(!window.grabWindow().isNull());QVERIFY(r.m_shaderInUse);
        r.setSize({1500,180});window.resize(1500,180);QVERIFY(!window.grabWindow().isNull());QVERIFY(!r.m_shaderInUse);QVERIFY(!r.m_plainInUse);
        for(const auto &atlas:r.m_tintedAtlases)QVERIFY(!atlas.isNull());
        r.setSize({320,180});window.resize(320,180);QVERIFY(!window.grabWindow().isNull());QVERIFY(r.m_shaderInUse);
        for(const auto &atlas:r.m_tintedAtlases)QVERIFY(atlas.isNull());
    }
    void textureMatchesPlainGeometry(){
        if(!gpu())QSKIP("Real OpenGL required");
        QQuickWindow window;window.resize(640,360);window.setColor(Qt::black);MatrixRenderer texture(window.contentItem());texture.setSize({640,360});deterministic(texture);
        window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));const auto first=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(texture.m_shaderInUse);texture.setVisible(false);
        MatrixRenderer plain(window.contentItem());plain.setSize({640,360});plain.m_supportCheck=[](QQuickWindow *w,int n,bool p){return p&&MatrixMaterial::supportsWindow(w,n,true);};deterministic(plain);
        const auto second=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(plain.m_plainInUse);
        quint64 sum=0;int maximum=0;for(int y=0;y<first.height();++y)for(int x=0;x<first.width()*4;++x){int d=std::abs(int(first.constScanLine(y)[x])-int(second.constScanLine(y)[x]));sum+=d;maximum=qMax(d,maximum);}
        qInfo()<<"Texture/plain MAE"<<double(sum)/(first.width()*first.height()*4)<<"maximum"<<maximum;
        QVERIFY(double(sum)/(first.width()*first.height()*4)<.01);QVERIFY(maximum<=3);
    }
    void oracleCaptures_data(){
        QTest::addColumn<int>("density");QTest::addColumn<double>("scale");QTest::addColumn<double>("speed");QTest::addColumn<double>("trails");QTest::addColumn<QString>("palette");QTest::addColumn<bool>("reduced");QTest::addColumn<QSize>("size");QTest::addColumn<QString>("background");
        QTest::newRow("matrix-owner-ocean")<<55<<2.0<<3.0<<1.0<<QStringLiteral("ocean")<<false<<QSize(3440,1440)<<QStringLiteral("black");
        QTest::newRow("matrix-owner-forest")<<55<<2.0<<3.0<<1.0<<QStringLiteral("forest")<<false<<QSize(3440,1440)<<QStringLiteral("black");
        QTest::newRow("matrix-minimum-ember")<<10<<.25<<.1<<0.0<<QStringLiteral("ember")<<false<<QSize(1920,1080)<<QStringLiteral("black");
        QTest::newRow("matrix-maximum-spectrum")<<100<<2.0<<3.0<<1.0<<QStringLiteral("spectrum")<<false<<QSize(2560,1440)<<QStringLiteral("black");
        QTest::newRow("matrix-reduced-forest")<<55<<2.0<<3.0<<1.0<<QStringLiteral("forest")<<true<<QSize(1280,720)<<QStringLiteral("#18091d");
        QTest::newRow("matrix-default-mono")<<50<<1.0<<1.0<<.35<<QStringLiteral("mono")<<false<<QSize(1280,720)<<QStringLiteral("#062f3c");
        QTest::newRow("matrix-default-pastel")<<50<<1.0<<1.0<<.35<<QStringLiteral("pastel")<<false<<QSize(1280,720)<<QStringLiteral("black");
    }
    void oracleCaptures(){
        if(!gpu())QSKIP("Pixel parity requires real OpenGL");
        QFETCH(int,density);QFETCH(double,scale);QFETCH(double,speed);QFETCH(double,trails);QFETCH(QString,palette);QFETCH(bool,reduced);QFETCH(QSize,size);QFETCH(QString,background);
        QQuickWindow window;window.setFlags(Qt::FramelessWindowHint);window.resize(size);window.setColor(QColor(background));QQmlEngine engine;
        QQmlComponent cc(&engine);cc.setData("import QtQuick; QtObject {property real animationDensity;property real animationScale;property real animationSpeed;property real trailAmount;property string animationPalette;property var presentationClock:null}",QUrl());
        std::unique_ptr<QObject> ctx(cc.create());QVERIFY(ctx);ctx->setProperty("animationDensity",density);ctx->setProperty("animationScale",scale*100);ctx->setProperty("animationSpeed",speed*100);ctx->setProperty("trailAmount",trails*100);ctx->setProperty("animationPalette",palette);
        QQmlComponent component(&engine,QUrl::fromLocalFile(QStringLiteral(MATRIX_ORACLE_DIR "/Matrix.qml")));QVERIFY2(component.isReady(),qPrintable(component.errorString()));
        std::unique_ptr<QObject> oracle(component.createWithInitialProperties({{QStringLiteral("context"),QVariant::fromValue(ctx.get())},{QStringLiteral("width"),size.width()},{QStringLiteral("height"),size.height()},{QStringLiteral("seed"),1},{QStringLiteral("reducedMotion"),reduced},{QStringLiteral("phase"),reduced?0.0:12.5*.18*speed}}));
        auto *item=qobject_cast<QQuickItem *>(oracle.get());QVERIFY(item);item->setParentItem(window.contentItem());window.show();QVERIFY(QTest::qWaitForWindowExposed(&window));
        auto before=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(!before.isNull());QCOMPARE(window.size(),size);QCOMPARE(before.size(),QSize(qRound(size.width()*window.effectiveDevicePixelRatio()),qRound(size.height()*window.effectiveDevicePixelRatio())));
        item->setVisible(false);MatrixRenderer renderer(window.contentItem());renderer.setSize(size);deterministic(renderer,density,scale,speed,trails,palette,1,reduced);auto after=window.grabWindow().convertToFormat(QImage::Format_RGBA8888);QVERIFY(renderer.m_shaderInUse);QCOMPARE(before.size(),after.size());
        double sum=0,squares=0,foregroundSum=0;quint64 different=0,foreground=0,nativeForeground=0;int maximum=0;QImage difference(before.size(),QImage::Format_RGBA8888);
        const QColor bg(background);
        for(int y=0;y<before.height();++y)for(int x=0;x<before.width();++x){
            const auto *a=before.constScanLine(y)+x*4,*b=after.constScanLine(y)+x*4;auto *d=difference.scanLine(y)+x*4;
            const bool ink=std::abs(int(a[0])-bg.red())+std::abs(int(a[1])-bg.green())+std::abs(int(a[2])-bg.blue())>15;
            for(int c=0;c<3;++c){const int e=std::abs(int(a[c])-int(b[c]));sum+=e;squares+=e*e;maximum=qMax(maximum,e);different+=e!=0;d[c]=uchar(qMin(255,e*8));if(ink)foregroundSum+=e;}d[3]=255;foreground+=ink;
            nativeForeground+=std::abs(int(b[0])-bg.red())+std::abs(int(b[1])-bg.green())+std::abs(int(b[2])-bg.blue())>15;
        }
        QVERIFY2(foreground>0,"Oracle capture has no visible glyph pixels");
        QVERIFY2(nativeForeground>0,"Native capture has no visible glyph pixels");
        const double channels=double(before.width())*before.height()*3;
        QJsonObject metrics{{QStringLiteral("mean_absolute_rgb"),sum/channels},{QStringLiteral("rmse_rgb"),std::sqrt(squares/channels)},{QStringLiteral("max_rgb"),maximum},{QStringLiteral("different_rgb_fraction"),double(different)/channels},{QStringLiteral("foreground_mean_absolute_rgb"),foregroundSum/std::max(1.0,double(foreground)*3)},{QStringLiteral("width"),before.width()},{QStringLiteral("height"),before.height()},{QStringLiteral("qt"),QString::fromLatin1(qVersion())}};
        QJsonArray fonts;for(const auto &glyph:renderer.m_glyphs){const QString family=glyph.font.familyName();bool found=false;for(const auto &entry:fonts)found|=entry.toString()==family;if(!found)fonts.append(family);}
        metrics.insert(QStringLiteral("resolved_fonts"),fonts);metrics.insert(QStringLiteral("atlas_glyphs"),int(renderer.m_glyphs.size()));
        metrics.insert(QStringLiteral("atlas_width"),renderer.m_atlas.width());metrics.insert(QStringLiteral("atlas_height"),renderer.m_atlas.height());
        qInfo().noquote()<<QJsonDocument(metrics).toJson(QJsonDocument::Compact);
        const QString dir=qEnvironmentVariable("VISUALS_CAPTURE_DIR");if(!dir.isEmpty()){
            QDir().mkpath(dir);const QString stem=dir+QStringLiteral("/")+QString::fromLatin1(QTest::currentDataTag());QVERIFY(before.save(stem+QStringLiteral("-old.png")));QVERIFY(after.save(stem+QStringLiteral("-new.png")));QVERIFY(difference.save(stem+QStringLiteral("-diff8x.png")));
            QImage pair(before.width()*2,before.height(),QImage::Format_RGB32);QPainter p(&pair);p.drawImage(0,0,before);p.drawImage(before.width(),0,after);p.end();QVERIFY(pair.save(stem+QStringLiteral("-side-by-side.png")));QFile file(stem+QStringLiteral("-metrics.json"));QVERIFY(file.open(QIODevice::WriteOnly));file.write(QJsonDocument(metrics).toJson());
        }
        // QtRendering uses distance fields and the old stream has a second
        // cached-layer filtering pass. Coverage is deliberately not bit-exact.
        // Keep both whole-image and foreground gates: sparse black backgrounds
        // must not conceal changed glyph faces, placement or excessive blur.
        QVERIFY2(sum/channels<1.5,"Glyph mean coverage error exceeded measured tolerance");
        QVERIFY2(std::sqrt(squares/channels)<8.0,"Glyph pixel RMSE exceeded measured tolerance");
        QVERIFY2(foregroundSum/std::max(1.0,double(foreground)*3)<24.0,"Foreground glyph error exceeded measured tolerance");
    }
};
QTEST_MAIN(MatrixRendererTest)
#include "test_matrixrenderer.moc"
