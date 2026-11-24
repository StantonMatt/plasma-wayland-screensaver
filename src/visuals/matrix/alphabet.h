// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "../common/glyphatlas.h"
const QImage& rainAlphabetImage();
// Keep font availability and fallback decisions local to Digital Rain.
void roundRainBraille(QImage&, const QRawFont&);
