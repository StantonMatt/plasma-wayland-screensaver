// SPDX-License-Identifier: GPL-3.0-or-later
// Drives the real qrc:/qml/Settings.qml with a real Configuration (in a
// throwaway file) and a stand-in for ApplicationController's QML API.
#include "configuration.h"
#include <QCoreApplication>
#include <QFile>
#include <QQmlContext>
#include <QQmlEngine>
#include <QTemporaryDir>
#include <QtQuickTest/quicktest.h>
#include <memory>

class FakeController final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int monitorCount MEMBER m_monitorCount NOTIFY monitorCountChanged)
    Q_PROPERTY(QString applicationVersion READ applicationVersion CONSTANT)
    Q_PROPERTY(QString applicationLicenseText READ applicationLicenseText CONSTANT)
    Q_PROPERTY(QString updateResult MEMBER m_updateResult)
    Q_PROPERTY(int previews MEMBER m_previews)
    Q_PROPERTY(int quits MEMBER m_quits)
    Q_PROPERTY(int updateChecks MEMBER m_updateChecks)
public:
    QString applicationVersion() const { return QStringLiteral("9.9.9"); }
    QString applicationLicenseText() const
    {
        QFile license(QStringLiteral(":/LICENSE"));
        if (!license.open(QIODevice::ReadOnly)) return QString();
        return QString::fromUtf8(license.readAll());
    }
    Q_INVOKABLE void Preview() { ++m_previews; }
    Q_INVOKABLE void Quit() { ++m_quits; }
    Q_INVOKABLE QString openUpdateCenter() { ++m_updateChecks; return m_updateResult; }
Q_SIGNALS:
    void monitorCountChanged();
    void previewFailed(const QString &reason);
private:
    int m_monitorCount = 3;
    QString m_updateResult = QStringLiteral("discover");
    int m_previews = 0;
    int m_quits = 0;
    int m_updateChecks = 0;
};

class SettingsQmlSetup : public QObject
{
    Q_OBJECT
    QTemporaryDir m_home;
    std::unique_ptr<Configuration> m_configuration;
    FakeController m_controller;
public:
    SettingsQmlSetup()
    {
        // Never read or write the desktop's configuration or session bus.
        qputenv("XDG_CONFIG_HOME", m_home.path().toUtf8());
        qputenv("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/pvs-settings-test-bus");
    }
public Q_SLOTS:
    void qmlEngineAvailable(QQmlEngine *engine)
    {
        if (!m_configuration)
            m_configuration = std::make_unique<Configuration>(m_home.filePath(QStringLiteral("settings-test-rc")));
        engine->rootContext()->setContextProperty(QStringLiteral("testConfig"), m_configuration.get());
        engine->rootContext()->setContextProperty(QStringLiteral("testController"), &m_controller);
    }
};

QUICK_TEST_MAIN_WITH_SETUP(settings, SettingsQmlSetup)
#include "test_settingsqml.moc"
