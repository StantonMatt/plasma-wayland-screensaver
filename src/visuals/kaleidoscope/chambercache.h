// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "visuals_depth_kaleidoscope.h"

// The mouth's event envelope can change even while poses and key light do not.
// Ordinary fold drift does not affect the cached chamber texture.
class KaleidoscopeMouthShadowState
{
public:
    bool sync(const kaleidoscope_lens& lens) noexcept
    {
        const float phase = lens.rim[3] == 0.f && lens.motion[2] < 0.f ? lens.motion[2] : 0.f;
        if (phase == m_phase)
            return false;
        m_phase = phase;
        return true;
    }
private:
    float m_phase = 0.f;
};
