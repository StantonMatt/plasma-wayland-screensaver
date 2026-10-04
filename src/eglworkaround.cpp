// SPDX-License-Identifier: GPL-3.0-or-later
#include "eglworkaround.h"

#include <QDebug>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>

namespace {
const QString wayland2 = QStringLiteral("09_nvidia_wayland2.json");
const QString legacyWayland = QStringLiteral("10_nvidia_wayland.json");
const QString filenames = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES");
const QString configDirs = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS");
const QString disableSync = QStringLiteral("__NV_DISABLE_EXPLICIT_SYNC");

QStringList libraryDirectories(const QProcessEnvironment &environment)
{
    QStringList paths = environment.value(QStringLiteral("LD_LIBRARY_PATH")).split(QLatin1Char(':'), Qt::SkipEmptyParts);
    paths << QStringLiteral("/lib/x86_64-linux-gnu") << QStringLiteral("/usr/lib/x86_64-linux-gnu")
          << QStringLiteral("/lib64") << QStringLiteral("/usr/lib64")
          << QStringLiteral("/lib") << QStringLiteral("/usr/lib");
    // Include native libraries installed outside the default search paths.
    // Read the loader's cache without dlopen: no EGL code runs before policy.
    QProcess cache;
    cache.start(QStringLiteral("/sbin/ldconfig"), {QStringLiteral("-p")}, QIODevice::ReadOnly);
    if (cache.waitForStarted(1000) && cache.waitForFinished(1000)
            && cache.exitStatus() == QProcess::NormalExit && cache.exitCode() == 0) {
        const auto lines = QString::fromLocal8Bit(cache.readAllStandardOutput()).split(QLatin1Char('\n'));
        for (const auto &line : lines) {
            const auto arrow = line.indexOf(QStringLiteral(" => "));
            if (arrow >= 0 && line.left(arrow).contains(QStringLiteral("x86-64"))) {
                paths << QFileInfo(line.mid(arrow + 4).trimmed()).absolutePath();
            }
        }
    }
    if (cache.state() != QProcess::NotRunning) {
        cache.kill();
        cache.waitForFinished(1000);
    }
    paths.removeDuplicates();
    return paths;
}
}

EglWorkaround::Inputs EglWorkaround::startupInputs(const QStringList &arguments,
                                                const QProcessEnvironment &environment)
{
    Inputs inputs{{environment.value(QStringLiteral("QT_QPA_PLATFORM")),
                   environment.value(QStringLiteral("QT_QUICK_BACKEND")),
                   environment.value(QStringLiteral("QSG_RHI_BACKEND")),
                   environment.value(QStringLiteral("PVS_GRAPHICS_API"))}, environment,
                  environment.value(QStringLiteral("PVS_KEEP_EGL_WAYLAND2")) == QStringLiteral("1")};
    // QGuiApplication consumes its platform arguments before the normal app
    // parser. Mirror its platform override and our startup flags beforehand.
    for (qsizetype i = 1; i < arguments.size(); ++i) {
        const auto &arg = arguments[i];
        if (arg == QStringLiteral("--")) break;
        if (arg == QStringLiteral("--keep-egl-wayland2") || arg == QStringLiteral("-keep-egl-wayland2")) {
            inputs.keepWayland2 = true;
        }
        for (const auto &option : {QStringLiteral("--graphics-api"), QStringLiteral("-graphics-api"),
                                   QStringLiteral("--platform"), QStringLiteral("-platform")}) {
            QString value;
            if (arg == option) {
                value = i + 1 < arguments.size() ? arguments[++i] : QString();
            } else if (arg.startsWith(option + QLatin1Char('='))) {
                value = arg.mid(option.size() + 1);
            } else {
                continue;
            }
            if (option.endsWith(QStringLiteral("graphics-api"))) inputs.graphics.requestedApi = value;
            else inputs.graphics.platform = value;
            break;
        }
    }
    return inputs;
}

bool EglWorkaround::eligible(const Inputs &inputs)
{
    return !inputs.keepWayland2
        && !inputs.environment.value(QStringLiteral("WAYLAND_DISPLAY")).isEmpty()
        && (inputs.graphics.platform.isEmpty() || inputs.graphics.platform.startsWith(QStringLiteral("wayland")))
        && GraphicsSelection::select(inputs.graphics, true) == GraphicsSelection::Api::OpenGL
        && !inputs.environment.contains(filenames)
        && !inputs.environment.contains(configDirs)
        && !inputs.environment.contains(disableSync);
}

QMap<QString, QString> EglWorkaround::changes(const Inputs &inputs, const QList<PlatformConfig> &configs)
{
    if (!eligible(inputs)) return {};
    bool hasWayland2 = false, hasLegacy = false;
    QStringList retained;
    for (const auto &config : configs) {
        const auto name = QFileInfo(config.path).fileName();
        if (name == wayland2) {
            hasWayland2 |= config.libraryExists;
        } else {
            retained << config.path;
            if (name == legacyWayland) hasLegacy |= config.libraryExists;
        }
    }
    if (!hasWayland2) return {};
    if (!hasLegacy) return {{disableSync, QStringLiteral("1")}};
    retained.removeDuplicates();
    return {{filenames, retained.join(QLatin1Char(':'))}};
}

QList<EglWorkaround::PlatformConfig> EglWorkaround::discoverConfigs(const QStringList &directories,
                                                               const QStringList &libraryDirectories)
{
    QList<PlatformConfig> configs;
    for (const auto &directory : directories) {
        const auto files = QDir(directory).entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Name);
        for (const auto &file : files) {
            bool libraryExists = false;
            if (file.fileName() == wayland2 || file.fileName() == legacyWayland) {
                QFile json(file.absoluteFilePath());
                if (json.open(QIODevice::ReadOnly)) {
                    const auto library = QJsonDocument::fromJson(json.readAll()).object()
                        .value(QStringLiteral("ICD")).toObject().value(QStringLiteral("library_path")).toString();
                    if (QDir::isAbsolutePath(library)) {
                        libraryExists = QFileInfo(library).isFile();
                    } else if (library.contains(QLatin1Char('/'))) {
                        libraryExists = QFileInfo(QDir(directory).filePath(library)).isFile();
                    } else if (!library.isEmpty()) {
                        for (const auto &path : libraryDirectories) {
                            if (QFileInfo(QDir(path).filePath(library)).isFile()) {
                                libraryExists = true;
                                break;
                            }
                        }
                    }
                }
            }
            configs.append({file.absoluteFilePath(), libraryExists});
        }
    }
    return configs;
}

void EglWorkaround::apply(int argc, char *argv[])
{
    QStringList arguments;
    for (int i = 0; i < argc; ++i) arguments << QString::fromLocal8Bit(argv[i]);
    const auto inputs = startupInputs(arguments, QProcessEnvironment::systemEnvironment());
    if (!eligible(inputs)) return;
    const QStringList directories{QStringLiteral("/usr/share/egl/egl_external_platform.d"),
                                  QStringLiteral("/etc/egl/egl_external_platform.d")};
    // Most systems do not install NVIDIA's plugin. Avoid querying ldconfig there.
    bool installed = false;
    for (const auto &directory : directories) installed |= QFileInfo::exists(QDir(directory).filePath(wayland2));
    if (!installed) return;
    const auto overrides = changes(inputs, discoverConfigs(directories, libraryDirectories(inputs.environment)));
    for (auto it = overrides.cbegin(); it != overrides.cend(); ++it) {
        if (qputenv(it.key().toLocal8Bit().constData(), it.value().toLocal8Bit())) {
            qInfo().noquote() << QStringLiteral("PVS NVIDIA EGL Wayland workaround: %1=%2").arg(it.key(), it.value());
        }
    }
}
