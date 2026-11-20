// SPDX-License-Identifier: GPL-3.0-or-later
#include "../src/inhibitor.h"

#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusContext>
#include <QDBusError>
#include <QSignalSpy>
#include <QTest>

class FakeScreenSaver final : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")

public:
    uint cookie = 0;
    bool reject = false;
    int inhibitCalls = 0;
    QList<uint> releasedCookies;
    QString appName;
    QString reason;

public Q_SLOTS:
    uint Inhibit(const QString &app, const QString &why)
    {
        ++inhibitCalls;
        appName = app;
        reason = why;
        if (reject) {
            sendErrorReply(QDBusError::Failed, QStringLiteral("ScreenSaver refused inhibition"));
        }
        return cookie;
    }

    void UnInhibit(uint value)
    {
        releasedCookies.append(value);
    }
};

class FakePowerManagement final : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.PowerManagement.Inhibit")

public:
    uint cookie = 42;
    bool reject = false;
    int inhibitCalls = 0;
    QList<uint> releasedCookies;
    QString appName;
    QString reason;

public Q_SLOTS:
    uint Inhibit(const QString &app, const QString &why)
    {
        ++inhibitCalls;
        appName = app;
        reason = why;
        if (reject) {
            sendErrorReply(QDBusError::Failed, QStringLiteral("PowerManagement refused inhibition"));
        }
        return cookie;
    }

    void UnInhibit(uint value)
    {
        releasedCookies.append(value);
    }
};

class InhibitorTest final : public QObject
{
    Q_OBJECT

private:
    FakeScreenSaver m_screenSaver;
    FakePowerManagement m_power;

    bool registerScreenSaver()
    {
        auto bus = QDBusConnection::sessionBus();
        return bus.registerObject(QStringLiteral("/ScreenSaver"), &m_screenSaver, QDBusConnection::ExportAllSlots)
            && bus.registerService(QStringLiteral("org.freedesktop.ScreenSaver"));
    }

    bool registerPowerManagement()
    {
        auto bus = QDBusConnection::sessionBus();
        return bus.registerObject(QStringLiteral("/org/freedesktop/PowerManagement/Inhibit"),
                                  &m_power, QDBusConnection::ExportAllSlots)
            && bus.registerService(QStringLiteral("org.freedesktop.PowerManagement"));
    }

private Q_SLOTS:
    void initTestCase()
    {
        const auto bus = QDBusConnection::sessionBus();
        QVERIFY(bus.isConnected());
        const auto portal = bus.interface()->isServiceRegistered(QStringLiteral("org.freedesktop.portal.Desktop"));
        QVERIFY(portal.isValid());
        QVERIFY(!portal.value());
    }

    void init()
    {
        m_screenSaver.cookie = 0;
        m_screenSaver.reject = false;
        m_screenSaver.inhibitCalls = 0;
        m_screenSaver.releasedCookies.clear();
        m_screenSaver.appName.clear();
        m_screenSaver.reason.clear();
        m_power.cookie = 42;
        m_power.reject = false;
        m_power.inhibitCalls = 0;
        m_power.releasedCookies.clear();
        m_power.appName.clear();
        m_power.reason.clear();
    }

    void cleanup()
    {
        auto bus = QDBusConnection::sessionBus();
        bus.unregisterService(QStringLiteral("org.freedesktop.ScreenSaver"));
        bus.unregisterService(QStringLiteral("org.freedesktop.PowerManagement"));
        bus.unregisterObject(QStringLiteral("/ScreenSaver"));
        bus.unregisterObject(QStringLiteral("/org/freedesktop/PowerManagement/Inhibit"));
    }

    void acquireAndRelease_data()
    {
        QTest::addColumn<uint>("screenSaverCookie");
        QTest::addColumn<uint>("powerCookie");
        QTest::newRow("zero-screen-saver-cookie") << uint(0) << uint(42);
        QTest::newRow("zero-power-cookie") << uint(17) << uint(0);
    }

    void acquireAndRelease()
    {
        QFETCH(uint, screenSaverCookie);
        QFETCH(uint, powerCookie);
        m_screenSaver.cookie = screenSaverCookie;
        m_power.cookie = powerCookie;
        QVERIFY(registerScreenSaver());
        QVERIFY(registerPowerManagement());
        Inhibitor inhibitor;
        QSignalSpy acquired(&inhibitor, &Inhibitor::acquired);
        QSignalSpy failed(&inhibitor, &Inhibitor::failed);

        inhibitor.acquire();
        QCOMPARE(acquired.count(), 1);
        QCOMPARE(failed.count(), 0);
        QVERIFY(inhibitor.isHeld());
        QCOMPARE(m_screenSaver.inhibitCalls, 1);
        QCOMPARE(m_power.inhibitCalls, 1);
        QCOMPARE(m_screenSaver.appName, QStringLiteral("plasma-visual-screensaver"));
        QCOMPARE(m_power.appName, m_screenSaver.appName);
        QCOMPARE(m_screenSaver.reason, QStringLiteral("Visual screensaver is active"));
        QCOMPARE(m_power.reason, m_screenSaver.reason);

        inhibitor.acquire();
        QCOMPARE(acquired.count(), 2);
        QCOMPARE(m_screenSaver.inhibitCalls, 1);
        QCOMPARE(m_power.inhibitCalls, 1);
        inhibitor.release();
        QVERIFY(!inhibitor.isHeld());
        QTRY_COMPARE(m_screenSaver.releasedCookies, QList<uint>{screenSaverCookie});
        QTRY_COMPARE(m_power.releasedCookies, QList<uint>{powerCookie});

        // A second release must not reuse either cookie.
        inhibitor.release();
        inhibitor.acquire();
        QCOMPARE(acquired.count(), 3);
        QCOMPARE(m_screenSaver.inhibitCalls, 2);
        QCOMPARE(m_power.inhibitCalls, 2);
        inhibitor.release();
        QTRY_COMPARE(m_screenSaver.releasedCookies, (QList<uint>{screenSaverCookie, screenSaverCookie}));
        QTRY_COMPARE(m_power.releasedCookies, (QList<uint>{powerCookie, powerCookie}));
    }

    void screenSaverFailure_data()
    {
        QTest::addColumn<bool>("servicePresent");
        QTest::newRow("absent") << false;
        QTest::newRow("method-rejected") << true;
    }

    void screenSaverFailure()
    {
        QFETCH(bool, servicePresent);
        if (servicePresent) {
            m_screenSaver.reject = true;
            QVERIFY(registerScreenSaver());
        }
        m_power.cookie = 0;
        QVERIFY(registerPowerManagement());
        Inhibitor inhibitor;
        QSignalSpy acquired(&inhibitor, &Inhibitor::acquired);
        QSignalSpy failed(&inhibitor, &Inhibitor::failed);

        inhibitor.acquire();
        QCOMPARE(acquired.count(), 0);
        QCOMPARE(failed.count(), 1);
        QVERIFY(!inhibitor.isHeld());
        QCOMPARE(m_power.inhibitCalls, 1);
        QTRY_COMPARE(m_power.releasedCookies, QList<uint>{0});
        QVERIFY(m_screenSaver.releasedCookies.isEmpty());
        const auto error = failed.first().first().toString();
        QCOMPARE(error, inhibitor.lastError());
        QVERIFY(error.contains(QStringLiteral("ScreenSaver inhibition failed:")));
        QVERIFY(error.contains(QStringLiteral("; "))); // Includes the portal failure.
        if (servicePresent) {
            QCOMPARE(m_screenSaver.inhibitCalls, 1);
            QVERIFY(error.contains(QStringLiteral("ScreenSaver refused inhibition")));
            m_screenSaver.reject = false;
        } else {
            QVERIFY(registerScreenSaver());
        }

        // Failure must leave acquisition reusable, without releasing the
        // rolled-back suspend cookie again during the next release.
        inhibitor.acquire();
        QCOMPARE(acquired.count(), 1);
        QCOMPARE(failed.count(), 1);
        QVERIFY(inhibitor.isHeld());
        inhibitor.release();
        QTRY_COMPARE(m_screenSaver.releasedCookies, QList<uint>{0});
        QTRY_COMPARE(m_power.releasedCookies, (QList<uint>{0, 0}));
    }

    void powerManagementFailure_data()
    {
        QTest::addColumn<bool>("servicePresent");
        QTest::newRow("absent") << false;
        QTest::newRow("method-rejected") << true;
    }

    void powerManagementFailure()
    {
        QFETCH(bool, servicePresent);
        if (servicePresent) {
            m_power.reject = true;
            QVERIFY(registerPowerManagement());
        }
        QVERIFY(registerScreenSaver());
        Inhibitor inhibitor;
        QSignalSpy acquired(&inhibitor, &Inhibitor::acquired);
        QSignalSpy failed(&inhibitor, &Inhibitor::failed);

        inhibitor.acquire();
        QCOMPARE(acquired.count(), 1);
        QCOMPARE(failed.count(), 0);
        QVERIFY(inhibitor.isHeld());
        QCOMPARE(m_screenSaver.inhibitCalls, 1);
        QCOMPARE(m_power.inhibitCalls, servicePresent ? 1 : 0);
        inhibitor.release();
        QVERIFY(!inhibitor.isHeld());
        QTRY_COMPARE(m_screenSaver.releasedCookies, QList<uint>{0});
        QVERIFY(m_power.releasedCookies.isEmpty());
    }

    void destructionReleasesCookiesOnce_data()
    {
        QTest::addColumn<bool>("releaseFirst");
        QTest::addColumn<bool>("screenSaverRejects");
        QTest::newRow("held") << false << false;
        QTest::newRow("already-released") << true << false;
        QTest::newRow("partial-failure") << false << true;
    }

    void destructionReleasesCookiesOnce()
    {
        QFETCH(bool, releaseFirst);
        QFETCH(bool, screenSaverRejects);
        m_screenSaver.reject = screenSaverRejects;
        m_power.cookie = 0;
        QVERIFY(registerScreenSaver());
        QVERIFY(registerPowerManagement());
        {
            Inhibitor inhibitor;
            inhibitor.acquire();
            QCOMPARE(inhibitor.isHeld(), !screenSaverRejects);
            if (releaseFirst) inhibitor.release();
        }
        QTRY_COMPARE(m_power.releasedCookies, QList<uint>{0});
        if (screenSaverRejects) {
            QVERIFY(m_screenSaver.releasedCookies.isEmpty());
        } else {
            QTRY_COMPARE(m_screenSaver.releasedCookies, QList<uint>{0});
        }
    }

    void bothServicesAbsent()
    {
        Inhibitor inhibitor;
        QSignalSpy acquired(&inhibitor, &Inhibitor::acquired);
        QSignalSpy failed(&inhibitor, &Inhibitor::failed);

        inhibitor.acquire();
        QCOMPARE(acquired.count(), 0);
        QCOMPARE(failed.count(), 1);
        QVERIFY(!inhibitor.isHeld());
        QVERIFY(inhibitor.lastError().contains(QStringLiteral("PowerManagement inhibition failed:")));
        QVERIFY(inhibitor.lastError().contains(QStringLiteral("ScreenSaver inhibition failed:")));
    }
};

QTEST_GUILESS_MAIN(InhibitorTest)
#include "test_inhibitor.moc"
