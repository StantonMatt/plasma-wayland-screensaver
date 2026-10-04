// SPDX-License-Identifier: GPL-3.0-or-later
#include <snakes_core.h>

#include <QTest>

class SnakesCoreAbiTest final : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void abiVersion()
    {
        QCOMPARE(snakes_core_abi_version(), uint32_t{3});
    }
};

QTEST_GUILESS_MAIN(SnakesCoreAbiTest)
#include "test_snakescore_abi.moc"
