// SPDX-License-Identifier: GPL-3.0-or-later
#include "applicationcontroller.h"

#include <QCommandLineOption>
#include <QCommandLineParser>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QGuiApplication>
#include <QIcon>

namespace {
constexpr auto serviceName = "org.kde.PlasmaVisualScreensaver";
constexpr auto objectPath = "/PlasmaVisualScreensaver";
constexpr auto interfaceName = "org.kde.PlasmaVisualScreensaver";

bool invokeExisting(const QString &method)
{
    QDBusInterface remote(QString::fromLatin1(serviceName), QString::fromLatin1(objectPath),
                          QString::fromLatin1(interfaceName), QDBusConnection::sessionBus());
    if (!remote.isValid()) {
        return false;
    }
    if (!method.isEmpty()) {
        remote.call(QDBus::NoBlock, method);
    }
    return true;
}
}

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QCoreApplication::setOrganizationDomain(QStringLiteral("kde.org"));
    QCoreApplication::setOrganizationName(QStringLiteral("KDE"));
    QCoreApplication::setApplicationName(QStringLiteral("plasma-visual-screensaver"));
    QCoreApplication::setApplicationVersion(QStringLiteral(APP_VERSION));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Plasma Visual Screensaver"));
    QGuiApplication::setDesktopFileName(QStringLiteral("org.kde.plasmavisualscreensaver"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.kde.plasmavisualscreensaver")));
    QGuiApplication::setQuitOnLastWindowClosed(false);

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("A non-locking visual screensaver for Plasma Wayland"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption background(QStringLiteral("background"),
                                        QStringLiteral("Run only as a background idle service"));
    const QCommandLineOption preview(QStringLiteral("preview"),
                                     QStringLiteral("Show the screensaver immediately"));
    const QCommandLineOption settings(QStringLiteral("settings"),
                                      QStringLiteral("Open the settings window"));
    const QCommandLineOption quit(QStringLiteral("quit"),
                                  QStringLiteral("Stop the running background process"));
    parser.addOptions({background, preview, settings, quit});
    parser.process(app);

    if (parser.isSet(quit)) {
        return invokeExisting(QStringLiteral("Quit")) ? 0 : 1;
    }
    const QString requestedMethod = parser.isSet(preview) ? QStringLiteral("Preview")
        : (parser.isSet(settings) || !parser.isSet(background)
               ? QStringLiteral("ShowSettings") : QString());
    if (invokeExisting(requestedMethod)) {
        return 0;
    }

    ApplicationController controller;
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.registerService(QString::fromLatin1(serviceName))) {
        qCritical() << "Could not register the single-instance D-Bus service:"
                    << bus.lastError().message();
        return 1;
    }
    if (!bus.registerObject(QString::fromLatin1(objectPath), &controller,
                            QDBusConnection::ExportScriptableSlots)) {
        qCritical() << "Could not export the control D-Bus object:" << bus.lastError().message();
        return 1;
    }

    controller.start();
    if (parser.isSet(preview)) {
        controller.Preview();
    } else if (!parser.isSet(background) || parser.isSet(settings)) {
        controller.ShowSettings();
    }
    return app.exec();
}
