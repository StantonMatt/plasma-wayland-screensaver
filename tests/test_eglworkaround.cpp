// SPDX-License-Identifier: GPL-3.0-or-later
#include "eglworkaround.h"

#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QTest>

namespace {
const QString filenames = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES");
const QString configDirs = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS");
const QString disableSync = QStringLiteral("__NV_DISABLE_EXPLICIT_SYNC");
const QString modern = QStringLiteral("/usr/share/egl/egl_external_platform.d/09_nvidia_wayland2.json");
const QString legacy = QStringLiteral("/usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json");
const QString mesa = QStringLiteral("/etc/egl/egl_external_platform.d/50_mesa.json");

EglWorkaround::Inputs inputs()
{
    QProcessEnvironment env;
    env.insert(QStringLiteral("WAYLAND_DISPLAY"), QStringLiteral("wayland-0"));
    return EglWorkaround::startupInputs({QStringLiteral("pvs")}, env);
}
}

class EglWorkaroundTest final : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void policy_data()
    {
        QTest::addColumn<QString>("scenario");
        QTest::addColumn<QString>("expectedVariable");
        QTest::addColumn<QString>("expectedValue");
        for (const auto &name : {"default", "wayland", "wayland-egl", "wayland-options", "explicit-opengl",
                                  "qt-opengl-precedence", "keep-zero", "only-modern",
                                  "legacy-missing-library", "legacy-json-absent"}) {
            QTest::newRow(name) << QString::fromLatin1(name) << disableSync << QStringLiteral("1");
        }
        for (const auto &name : {"no-plugins", "non-nvidia", "legacy-only", "modern-missing-library", "xcb", "offscreen",
                                  "minimal", "no-wayland-display", "empty-wayland-display", "pvs-vulkan",
                                  "qt-vulkan", "software", "custom-quick", "unknown-rhi", "invalid-api",
                                  "keep-env", "keep-cli", "filenames", "filenames-empty", "dirs", "dirs-empty",
                                  "disable-sync", "disable-sync-zero", "disable-sync-empty"}) {
            QTest::newRow(name) << QString::fromLatin1(name) << QString() << QString();
        }
    }

    void policy()
    {
        QFETCH(QString, scenario);
        QFETCH(QString, expectedVariable);
        QFETCH(QString, expectedValue);
        auto in = inputs();
        QList<EglWorkaround::PlatformConfig> configs{{modern, true}, {legacy, true}, {mesa, false}};
        if (scenario == QStringLiteral("wayland")) in.graphics.platform = QStringLiteral("wayland");
        if (scenario == QStringLiteral("wayland-egl")) in.graphics.platform = QStringLiteral("wayland-egl");
        if (scenario == QStringLiteral("wayland-options")) in.graphics.platform = QStringLiteral("wayland:foo");
        if (scenario == QStringLiteral("explicit-opengl")) in.graphics.requestedApi = QStringLiteral("opengl");
        if (scenario == QStringLiteral("qt-opengl-precedence")) {
            in.graphics.rhiBackend = QStringLiteral("opengl");
            in.graphics.requestedApi = QStringLiteral("vulkan");
        }
        if (scenario == QStringLiteral("only-modern")) configs = {{modern, true}};
        if (scenario == QStringLiteral("legacy-missing-library")) configs[1].libraryExists = false;
        if (scenario == QStringLiteral("legacy-json-absent")) configs.removeAt(1);
        if (scenario == QStringLiteral("no-plugins")) configs = {{mesa, false}};
        if (scenario == QStringLiteral("non-nvidia")) configs = {{mesa, true}};
        if (scenario == QStringLiteral("legacy-only")) configs.removeAt(0);
        if (scenario == QStringLiteral("modern-missing-library")) configs[0].libraryExists = false;
        if (scenario == QStringLiteral("xcb") || scenario == QStringLiteral("offscreen") || scenario == QStringLiteral("minimal")) {
            in.graphics.platform = scenario;
        }
        if (scenario == QStringLiteral("no-wayland-display")) in.environment.remove(QStringLiteral("WAYLAND_DISPLAY"));
        if (scenario == QStringLiteral("empty-wayland-display")) in.environment.insert(QStringLiteral("WAYLAND_DISPLAY"), QString());
        if (scenario == QStringLiteral("pvs-vulkan")) in.graphics.requestedApi = QStringLiteral("vulkan");
        if (scenario == QStringLiteral("qt-vulkan")) in.graphics.rhiBackend = QStringLiteral("vulkan");
        if (scenario == QStringLiteral("software")) in.graphics.quickBackend = QStringLiteral("software");
        if (scenario == QStringLiteral("custom-quick")) in.graphics.quickBackend = QStringLiteral("custom");
        if (scenario == QStringLiteral("unknown-rhi")) in.graphics.rhiBackend = QStringLiteral("custom");
        if (scenario == QStringLiteral("invalid-api")) in.graphics.requestedApi = QStringLiteral("invalid");
        if (scenario == QStringLiteral("keep-cli")) in.keepWayland2 = true;
        if (scenario == QStringLiteral("keep-env") || scenario == QStringLiteral("keep-zero")) {
            auto env = in.environment;
            env.insert(QStringLiteral("PVS_KEEP_EGL_WAYLAND2"), scenario == QStringLiteral("keep-env") ? QStringLiteral("1") : QStringLiteral("0"));
            in = EglWorkaround::startupInputs({QStringLiteral("pvs")}, env);
        }
        for (const auto &pair : {qMakePair(QStringLiteral("filenames"), filenames),
                                 qMakePair(QStringLiteral("dirs"), configDirs), qMakePair(QStringLiteral("disable-sync"), disableSync)}) {
            if (scenario == pair.first) in.environment.insert(pair.second, QStringLiteral("user-value"));
            if (scenario == pair.first + QStringLiteral("-empty")) in.environment.insert(pair.second, QString());
            if (scenario == pair.first + QStringLiteral("-zero")) in.environment.insert(pair.second, QStringLiteral("0"));
        }
        QMap<QString, QString> expected;
        if (!expectedVariable.isEmpty()) expected.insert(expectedVariable, expectedValue);
        QCOMPARE(EglWorkaround::changes(in, configs), expected);
    }

    void bothDirectoriesAndOtherPlatforms()
    {
        const QString etcModern = QStringLiteral("/etc/egl/egl_external_platform.d/09_nvidia_wayland2.json");
        const QString etcLegacy = QStringLiteral("/etc/egl/egl_external_platform.d/10_nvidia_wayland.json");
        const QString gbm = QStringLiteral("/usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json");
        const QList<EglWorkaround::PlatformConfig> configs{{modern, true}, {gbm, false}, {etcModern, true},
                                                          {etcLegacy, true}, {mesa, false}, {mesa, false}};
        const QMap<QString, QString> expected{{disableSync, QStringLiteral("1")}};
        QCOMPARE(EglWorkaround::changes(inputs(), configs), expected);
        // A broken copy in either directory must not hide a usable copy.
        auto mixedConfigs = configs;
        mixedConfigs[0].libraryExists = false;
        QCOMPARE(EglWorkaround::changes(inputs(), mixedConfigs), expected);
        mixedConfigs[0].libraryExists = true;
        mixedConfigs[2].libraryExists = false;
        QCOMPARE(EglWorkaround::changes(inputs(), mixedConfigs), expected);
        mixedConfigs[0].libraryExists = false;
        QVERIFY(EglWorkaround::changes(inputs(), mixedConfigs).isEmpty());
    }

    void startupArguments_data()
    {
        QTest::addColumn<QStringList>("args");
        QTest::addColumn<QString>("api");
        QTest::addColumn<QString>("platform");
        QTest::addColumn<bool>("keep");
        QTest::newRow("environment") << QStringList{QStringLiteral("pvs")} << QStringLiteral("vulkan") << QStringLiteral("wayland") << false;
        QTest::newRow("opengl-cli") << QStringList{QStringLiteral("pvs"), QStringLiteral("--graphics-api"), QStringLiteral("opengl")} << QStringLiteral("opengl") << QStringLiteral("wayland") << false;
        QTest::newRow("equals") << QStringList{QStringLiteral("pvs"), QStringLiteral("--graphics-api=opengl"), QStringLiteral("--keep-egl-wayland2")} << QStringLiteral("opengl") << QStringLiteral("wayland") << true;
        QTest::newRow("single-dash") << QStringList{QStringLiteral("pvs"), QStringLiteral("-graphics-api=opengl"), QStringLiteral("-keep-egl-wayland2")} << QStringLiteral("opengl") << QStringLiteral("wayland") << true;
        QTest::newRow("last-value") << QStringList{QStringLiteral("pvs"), QStringLiteral("--graphics-api"), QStringLiteral("vulkan"), QStringLiteral("--graphics-api"), QStringLiteral("opengl")} << QStringLiteral("opengl") << QStringLiteral("wayland") << false;
        QTest::newRow("end-options") << QStringList{QStringLiteral("pvs"), QStringLiteral("--"), QStringLiteral("--graphics-api=opengl"), QStringLiteral("--keep-egl-wayland2")} << QStringLiteral("vulkan") << QStringLiteral("wayland") << false;
        QTest::newRow("qt-platform") << QStringList{QStringLiteral("pvs"), QStringLiteral("-platform"), QStringLiteral("xcb")} << QStringLiteral("vulkan") << QStringLiteral("xcb") << false;
        QTest::newRow("qt-platform-equals") << QStringList{QStringLiteral("pvs"), QStringLiteral("--platform=offscreen")} << QStringLiteral("vulkan") << QStringLiteral("offscreen") << false;
        QTest::newRow("value-is-not-flag") << QStringList{QStringLiteral("pvs"), QStringLiteral("--graphics-api"), QStringLiteral("--keep-egl-wayland2")} << QStringLiteral("--keep-egl-wayland2") << QStringLiteral("wayland") << false;
    }

    void startupArguments()
    {
        QFETCH(QStringList, args);
        QFETCH(QString, api);
        QFETCH(QString, platform);
        QFETCH(bool, keep);
        auto env = inputs().environment;
        env.insert(QStringLiteral("PVS_GRAPHICS_API"), QStringLiteral("vulkan"));
        env.insert(QStringLiteral("QT_QPA_PLATFORM"), QStringLiteral("wayland"));
        const auto in = EglWorkaround::startupInputs(args, env);
        QCOMPARE(in.graphics.requestedApi, api);
        QCOMPARE(in.graphics.platform, platform);
        QCOMPARE(in.keepWayland2, keep);
    }

    void discovery_data()
    {
        QTest::addColumn<QString>("kind");
        QTest::addColumn<bool>("valid");
        for (const auto &kind : {"absolute", "relative", "soname", "symlink"}) {
            QTest::newRow(kind) << QString::fromLatin1(kind) << true;
        }
        for (const auto &kind : {"missing", "directory", "broken-symlink", "malformed", "no-icd", "no-library"}) {
            QTest::newRow(kind) << QString::fromLatin1(kind) << false;
        }
    }

    void discovery()
    {
        QFETCH(QString, kind);
        QFETCH(bool, valid);
        QTemporaryDir tmp;
        QVERIFY(tmp.isValid());
        QDir root(tmp.path());
        QVERIFY(root.mkdir(QStringLiteral("config")));
        QVERIFY(root.mkdir(QStringLiteral("lib")));
        const auto library = root.filePath(QStringLiteral("lib/actual.so"));
        QFile lib(library);
        QVERIFY(lib.open(QIODevice::WriteOnly));
        lib.close();
        QString reference = library;
        if (kind == QStringLiteral("relative")) reference = QStringLiteral("../lib/actual.so");
        if (kind == QStringLiteral("soname")) reference = QStringLiteral("actual.so");
        if (kind == QStringLiteral("missing")) reference = root.filePath(QStringLiteral("missing.so"));
        if (kind == QStringLiteral("directory")) reference = root.filePath(QStringLiteral("lib"));
        if (kind == QStringLiteral("symlink") || kind == QStringLiteral("broken-symlink")) {
            reference = root.filePath(QStringLiteral("lib/link.so"));
            QVERIFY(QFile::link(kind == QStringLiteral("symlink") ? library : library + QStringLiteral(".missing"), reference));
        }
        QByteArray json = QJsonDocument(QJsonObject{{QStringLiteral("ICD"), QJsonObject{{QStringLiteral("library_path"), reference}}}}).toJson();
        if (kind == QStringLiteral("malformed")) json = "{broken";
        if (kind == QStringLiteral("no-icd")) json = "{}";
        if (kind == QStringLiteral("no-library")) json = "{\"ICD\":{}}";
        const auto path = root.filePath(QStringLiteral("config/09_nvidia_wayland2.json"));
        QFile config(path);
        QVERIFY(config.open(QIODevice::WriteOnly));
        QCOMPARE(config.write(json), json.size());
        config.close();
        const auto configs = EglWorkaround::discoverConfigs({root.filePath(QStringLiteral("absent")), root.filePath(QStringLiteral("config"))},
                                                           {root.filePath(QStringLiteral("lib"))});
        QCOMPARE(configs.size(), 1);
        QCOMPARE(configs.front().path, path);
        QCOMPARE(configs.front().libraryExists, valid);
    }
};
QTEST_APPLESS_MAIN(EglWorkaroundTest)
#include "test_eglworkaround.moc"
