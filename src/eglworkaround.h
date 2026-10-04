// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include "graphicsbackend.h"
#include <QMap>
#include <QProcessEnvironment>
#include <QStringList>

namespace EglWorkaround {
struct Inputs {
    GraphicsSelection::Inputs graphics;
    QProcessEnvironment environment;
    bool keepWayland2 = false;
};
struct PlatformConfig {
    QString path;
    bool libraryExists = false;
};

// Pure startup policy. Explicitly set environment variables, including empty
// values, belong to the user. No filesystem or process environment access.
Inputs startupInputs(const QStringList &arguments, const QProcessEnvironment &environment);
bool eligible(const Inputs &inputs);
QMap<QString, QString> changes(const Inputs &inputs, const QList<PlatformConfig> &configs);

// Inventory is kept separate from policy so missing/broken installations can
// be tested without loading an EGL library or requiring a desktop session.
QList<PlatformConfig> discoverConfigs(const QStringList &directories, const QStringList &libraryDirectories);
void apply(int argc, char *argv[]);
}
