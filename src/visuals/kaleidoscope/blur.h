// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_kaleidoscope.h"
#include <QSize>
#include <algorithm>
#include <array>
#include <optional>

// Blur uniforms outlive worlds when the offscreen dimensions stay clamped.
class KaleidoscopeBlurUniforms
{
public:
    bool sync(const kaleidoscope_lens& lens)
    {
        const std::array<float, 3> inputs { lens.eye[2], lens.glass[0], lens.glass[1] };
        m_dirty |= inputs != m_inputs;
        m_inputs = inputs;
        return m_dirty;
    }
    void invalidate() { m_dirty = true; }
    std::optional<std::array<std::array<float, 4>, 2>> take(QSize size, float flip)
    {
        if (!m_dirty)
            return std::nullopt;
        std::array<std::array<float, 4>, 2> uniforms;
        for (size_t i = 0; i < uniforms.size(); ++i) {
            const float spread = (i ? 0.55f : 0.45f) * (2.f + 12.f * m_inputs[1])
                * 1.67f * size.width() / (2.10f * m_inputs[0] * m_inputs[2]);
            uniforms[i] = { spread / size.width(), spread / size.height(), flip, 0 };
            size = { std::max(1, size.width() / 2), std::max(1, size.height() / 2) };
        }
        m_dirty = false;
        return uniforms;
    }

private:
    std::array<float, 3> m_inputs {};
    bool m_dirty = true;
};
