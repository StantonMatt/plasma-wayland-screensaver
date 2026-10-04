// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

namespace EglDrain {
// Shared by pre-EGL policy and the runtime caller guard; no library is loaded.
const char *supportedVersion(const char *canonicalPath);
// Set once, before QGuiApplication / EGL initialization. The interposer only
// registers queues from the exact canonical library selected at startup.
bool enable(const char *canonicalPath);
}
