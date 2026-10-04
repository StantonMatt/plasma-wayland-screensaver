// SPDX-License-Identifier: GPL-3.0-or-later
#include "eglworkaround.h"
#include "egldrain.h"

#include <QDebug>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>

namespace {
const QString wayland2 = QStringLiteral("09_nvidia_wayland2.json");
const QString filenames = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES");
const QString configDirs = QStringLiteral("__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS");
const QString disableSync = QStringLiteral("__NV_DISABLE_EXPLICIT_SYNC");

struct LibrarySearch {
    QStringList directories;
    QMap<QString, QString> cache;
    bool drainSafe = true;
};

bool nativeLibrary(const QString &path)
{
    if (path.isEmpty()) return false;
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) return false;
    const auto header = file.read(20);
    // A loader search can skip a wrong-architecture file and continue to a
    // different version. Only trust ELF64 little-endian x86-64 shared objects.
    return header.size() == 20 && header.startsWith("\177ELF")
        && header[4] == 2 && header[5] == 1 && header[6] == 1
        && header[16] == 3 && header[17] == 0 && header[18] == 62 && header[19] == 0;
}

LibrarySearch librarySearch(const QProcessEnvironment &environment)
{
    LibrarySearch result;
    // dlopen searches LD_LIBRARY_PATH before the exact SONAME cache entry.
    // Do not approximate the cache by searching all its directories: that can
    // choose a different version from the one the dynamic loader will use.
    if (environment.contains(QStringLiteral("LD_LIBRARY_PATH"))) {
        result.directories = environment.value(QStringLiteral("LD_LIBRARY_PATH")).split(QLatin1Char(':'));
        for (const auto &path : result.directories) {
            if (!QDir::isAbsolutePath(path) || path.contains(QLatin1Char('$')) || path.contains(QLatin1Char(';')))
                result.drainSafe = false;
            // HWCAP subdirectories can precede the base file. Fail safely
            // instead of trying to reproduce glibc's CPU capability selection.
            if (QFileInfo::exists(QDir(path).filePath(QStringLiteral("glibc-hwcaps")))) result.drainSafe = false;
        }
    }
    for (const auto &key : {"LD_PRELOAD", "LD_AUDIT"}) {
        if (!environment.value(QString::fromLatin1(key)).isEmpty()) result.drainSafe = false;
    }
    QProcess cache;
    cache.start(QStringLiteral("/sbin/ldconfig"), {QStringLiteral("-p")}, QIODevice::ReadOnly);
    if (cache.waitForStarted(1000) && cache.waitForFinished(1000)
            && cache.exitStatus() == QProcess::NormalExit && cache.exitCode() == 0) {
        const auto lines = QString::fromLocal8Bit(cache.readAllStandardOutput()).split(QLatin1Char('\n'));
        for (const auto &line : lines) {
            const auto arrow = line.indexOf(QStringLiteral(" => "));
            if (arrow < 0 || !line.left(arrow).contains(QStringLiteral("x86-64"))) continue;
            const auto soname = line.trimmed().section(QLatin1Char(' '), 0, 0);
            const auto path = line.mid(arrow + 4).trimmed();
            if (line.left(arrow).contains(QStringLiteral("hwcap"))) result.drainSafe = false;
            if (!result.cache.contains(soname)) result.cache.insert(soname, path);
        }
    } else {
        result.drainSafe = false;
    }
    if (cache.state() != QProcess::NotRunning) {
        cache.kill();
        cache.waitForFinished(1000);
    }
    // glibc's native fallback directories follow the cache.
    result.directories.removeDuplicates();
    return result;
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

EglWorkaround::LeakFix EglWorkaround::selectedFix(const Inputs &inputs, const QList<PlatformConfig> &configs)
{
    if (!eligible(inputs)) return LeakFix::Off;
    const auto mode = inputs.environment.value(QStringLiteral("PVS_EGL_LEAK_FIX"), QStringLiteral("drain"));
    // Unknown/empty explicit values preserve the established opt-out behavior.
    if (mode != QStringLiteral("noexplicit") && mode != QStringLiteral("drain")) return LeakFix::Off;
    QString resolved;
    bool found = false;
    bool safe = true;
    for (const auto &config : configs) {
        if (config.libraryExists && (QFileInfo(config.path).fileName() == wayland2
                || QFileInfo(config.resolvedLibrary).fileName().startsWith(QStringLiteral("libnvidia-egl-wayland2.so.")))) {
            found = true;
            safe &= config.drainSafe && EglDrain::supportedVersion(config.resolvedLibrary.toLocal8Bit().constData());
            if (!resolved.isEmpty() && resolved != config.resolvedLibrary) safe = false;
            resolved = config.resolvedLibrary;
        }
    }
    if (!found) return LeakFix::Off;
    // Even an explicit drain request falls back when its runtime caller guard
    // would reject the resolved version. EGL cannot change sync policy later.
    return mode == QStringLiteral("drain") && safe ? LeakFix::Drain : LeakFix::NoExplicit;
}

QMap<QString, QString> EglWorkaround::changes(const Inputs &inputs, const QList<PlatformConfig> &configs)
{
    if (selectedFix(inputs, configs) == LeakFix::NoExplicit) return {{disableSync, QStringLiteral("1")}};
    return {};
}

QList<EglWorkaround::PlatformConfig> EglWorkaround::discoverConfigs(const QStringList &directories,
                                                               const QStringList &libraryDirectories,
                                                               const QMap<QString, QString> &cachedLibraries,
                                                               bool drainSafe)
{
    QList<PlatformConfig> configs;
    for (const auto &directory : directories) {
        const auto files = QDir(directory).entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Name);
        for (const auto &file : files) {
            QString resolved;
            bool safe = drainSafe;
            QFile json(file.absoluteFilePath());
            if (json.open(QIODevice::ReadOnly)) {
                const auto library = QJsonDocument::fromJson(json.readAll()).object()
                    .value(QStringLiteral("ICD")).toObject().value(QStringLiteral("library_path")).toString();
                if (QDir::isAbsolutePath(library)) {
                    if (QFileInfo(library).isFile()) resolved = QFileInfo(library).canonicalFilePath();
                } else if (library.contains(QLatin1Char('/'))) {
                    // Relative dlopen paths are not a verified loader contract.
                    safe = false;
                    if (QFileInfo(QDir(directory).filePath(library)).isFile())
                        resolved = QFileInfo(QDir(directory).filePath(library)).canonicalFilePath();
                } else if (!library.isEmpty()) {
                    for (const auto &path : libraryDirectories) {
                        const QFileInfo candidate(QDir(path).filePath(library));
                        if (candidate.isFile()) { resolved = candidate.canonicalFilePath(); break; }
                    }
                    if (resolved.isEmpty() && QFileInfo(cachedLibraries.value(library)).isFile())
                        resolved = QFileInfo(cachedLibraries.value(library)).canonicalFilePath();
                    if (resolved.isEmpty()) {
                        for (const auto &path : {"/lib/x86_64-linux-gnu", "/usr/lib/x86_64-linux-gnu", "/lib64", "/usr/lib64", "/lib", "/usr/lib"}) {
                            const QFileInfo candidate(QDir(QString::fromLatin1(path)).filePath(library));
                            if (candidate.isFile()) { resolved = candidate.canonicalFilePath(); break; }
                        }
                    }
                }
            }
            configs.append({file.absoluteFilePath(), !resolved.isEmpty(), resolved, safe && nativeLibrary(resolved)});
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
    // Most non-NVIDIA systems have no external-platform configs. A renamed
    // JSON must still be recognized by its resolved NVIDIA library path.
    bool hasConfigs = false;
    for (const auto &directory : directories)
        hasConfigs |= !QDir(directory).entryList({QStringLiteral("*.json")}, QDir::Files).isEmpty();
    if (!hasConfigs) return;
    const auto search = librarySearch(inputs.environment);
    const auto configs = discoverConfigs(directories, search.directories, search.cache, search.drainSafe);
    if (selectedFix(inputs, configs) == LeakFix::Drain) {
        for (const auto &config : configs) {
            if (EglDrain::supportedVersion(config.resolvedLibrary.toLocal8Bit().constData())) {
                const auto library = config.resolvedLibrary.toLocal8Bit();
                if (EglDrain::enable(library.constData())) {
                    qInfo().noquote() << "PVS NVIDIA EGL Wayland workaround: drain selected before EGL initialization:"
                                     << config.resolvedLibrary << "(explicit sync unchanged; expect active wl_surface log for each rendering surface)";
                    return;
                }
            }
        }
        // Cannot normally fail: selectedFix and enable share the same guard.
        qWarning("PVS EGL drain: preflight failed; using noexplicit before EGL initialization");
        if (qputenv("__NV_DISABLE_EXPLICIT_SYNC", "1"))
            qInfo("PVS NVIDIA EGL Wayland workaround: __NV_DISABLE_EXPLICIT_SYNC=1");
        return;
    }
    const auto overrides = changes(inputs, configs);
    if (!overrides.isEmpty() && inputs.environment.value(QStringLiteral("PVS_EGL_LEAK_FIX")) != QStringLiteral("noexplicit"))
        qInfo("PVS EGL drain: unverified version or loader resolution; using noexplicit before EGL initialization");
    for (auto it = overrides.cbegin(); it != overrides.cend(); ++it) {
        if (qputenv(it.key().toLocal8Bit().constData(), it.value().toLocal8Bit())) {
            qInfo().noquote() << QStringLiteral("PVS NVIDIA EGL Wayland workaround: %1=%2").arg(it.key(), it.value());
        }
    }
}
