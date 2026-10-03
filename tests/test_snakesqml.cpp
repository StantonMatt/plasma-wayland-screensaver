// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakesimulation.h"
#include <QQmlContext>
#include <QQmlEngine>
#include <QtQuickTest/quicktest.h>

class SnakeQmlSetup : public QObject
{
    Q_OBJECT
    SnakeSimulation simulation{{640, 360, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, 0}};
public Q_SLOTS:
    void applicationAvailable() { registerSnakeTypes(); }
    void qmlEngineAvailable(QQmlEngine *engine)
    {
        simulation.advance(1.0 / 30);
        engine->rootContext()->setContextProperty(QStringLiteral("snakeTestSimulation"), &simulation);
    }
};
QUICK_TEST_MAIN_WITH_SETUP(snakes, SnakeQmlSetup)
#include "test_snakesqml.moc"
