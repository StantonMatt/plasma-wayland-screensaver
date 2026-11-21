// SPDX-License-Identifier: GPL-3.0-or-later
// Offline analysis of decoded RGBA8 frames. For the local rule, a pixel has
// to complete two opposing transitions. Coalesce completions over three frames
// before counting concurrent area against the one-second frequency limit.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <deque>
#include <vector>

struct GeneralFlash {
    int16_t low = 0, high = 0;
    bool pending = false;
    int8_t direction = 0;
    bool sample(int level)
    {
        int next = 0;
        if (direction <= 0 && level - low >= 410 && low < 3277)
            next = 1;
        else if (direction >= 0 && high - level >= 410 && level < 3277)
            next = -1;
        if (next) {
            direction = int8_t(next);
            low = high = int16_t(level);
            pending = !pending;
            return !pending;
        }
        low = std::min(low, int16_t(level));
        high = std::max(high, int16_t(level));
        return false;
    }
};
struct Chromaticity {
    float u, v;
    float distance2(Chromaticity b) const { return (u - b.u) * (u - b.u) + (v - b.v) * (v - b.v); }
    static Chromaticity fromLinear(float r, float g, float b)
    {
        const float x = .4124564f * r + .3575761f * g + .1804375f * b;
        const float y = .2126729f * r + .7151522f * g + .0721750f * b;
        const float z = .0193339f * r + .1191920f * g + .9503041f * b;
        const float d = x + 15 * y + 3 * z;
        return d > 0 ? Chromaticity { 4 * x / d, 9 * y / d } : Chromaticity { 0, 0 };
    }
};
// WCAG's saturated-red transition uses a distance >0.2 in CIE 1976 UCS.
// Keep a turning point on each side so a gradual red/neutral fade is counted.
struct RedFlash {
    Chromaticity extreme {}, opposite { .450704f, .522887f }; // sRGB red
    bool redSide = false, pending = false;
    bool sample(Chromaticity colour, bool saturated)
    {
        if (redSide == saturated) {
            if (colour.distance2(opposite) > extreme.distance2(opposite))
                extreme = colour;
        } else if (colour.distance2(extreme) > .2f * .2f) {
            opposite = extreme;
            extreme = colour;
            redSide = saturated;
            pending = !pending;
            return !pending;
        }
        return false;
    }
};
struct Pixel {
    GeneralFlash general;
    RedFlash red;
};

struct Field {
    int x, y, width, height, windowWidth, windowHeight;
    bool qualifies(const std::vector<uint8_t>& mask, int stride, int changes, int& left, int& top) const
    {
        const int limit = windowWidth * windowHeight / 4;
        if (changes <= limit)
            return false;
        // A seam detail may show only part of a monitor's 10° window.
        // Keep the threshold for the full field, including its unseen part.
        const int w = std::min(windowWidth, width), h = std::min(windowHeight, height);
        std::vector<int> columns(width);
        for (int row = 0; row < height; ++row) {
            const auto* add = mask.data() + size_t(y + row) * stride + x;
            for (int i = 0; i < width; ++i)
                columns[i] += add[i];
            if (row >= h) {
                const auto* remove = add - size_t(h) * stride;
                for (int i = 0; i < width; ++i)
                    columns[i] -= remove[i];
            }
            if (row + 1 < h)
                continue;
            int sum = 0;
            for (int i = 0; i < width; ++i) {
                sum += columns[i];
                if (i >= w)
                    sum -= columns[i - w];
                if (i + 1 >= w && sum > limit) {
                    left = x + i + 1 - w;
                    top = y + row + 1 - h;
                    return true;
                }
            }
        }
        return false;
    }
};

int main(int argc, char** argv)
{
    if (argc < 4 || (argc - 4) % 6 != 0)
        return 2;
    const int width = std::atoi(argv[1]), height = std::atoi(argv[2]), fps = std::atoi(argv[3]);
    if (width < 1 || height < 1 || width > 16384 || height > 16384 || fps < 6 || fps > 240)
        return 2;
    std::vector<Field> fields;
    for (int i = 4; i < argc; i += 6) {
        Field f { std::atoi(argv[i]), std::atoi(argv[i + 1]), std::atoi(argv[i + 2]), std::atoi(argv[i + 3]),
            std::atoi(argv[i + 4]), std::atoi(argv[i + 5]) };
        if (f.x < 0 || f.y < 0 || f.width < 1 || f.height < 1 || f.width > width || f.height > height
            || f.x > width - f.width || f.y > height - f.height || f.windowWidth < 1 || f.windowHeight < 1
            || f.windowWidth > 16384 || f.windowHeight > 16384)
            return 2;
        fields.push_back(f);
    }
    if (fields.empty())
        fields.push_back({ 0, 0, width, height, std::max(1, int(std::lround(width * 341. / 1024))),
            std::max(1, int(std::lround(height * 256. / 768))) });
    const size_t count = size_t(width) * height;
    std::array<std::array<int, 256>, 3> lut {};
    std::array<int, 256> linearChannel {};
    constexpr std::array weights { 0.2126, 0.7152, 0.0722 };
    for (int i = 0; i < 256; ++i) {
        const double s = i / 255.;
        const double linear = s <= 0.04045 ? s / 12.92 : std::pow((s + 0.055) / 1.055, 2.4);
        linearChannel[i] = int(std::round(4096 * linear));
        for (int c = 0; c < 3; ++c)
            lut[c][i] = int(std::round(4096 * weights[c] * linear));
    }
    std::vector<uint32_t> pixels(count), last(count);
    std::vector<Pixel> states(count);
    constexpr int coalescingFrames = 3;
    std::vector<uint8_t> mask(count), completions(count);
    std::array<std::vector<size_t>, coalescingFrames> completionFrames;
    int activePixels = 0, lastFlash = -coalescingFrames;
    int hits = 0, qualified = 0, peak = 0;
    int firstFrame = -1, firstX = -1, firstY = -1;
    double maxSwing = 0.;
    uint64_t sum = 0;
    std::deque<double> recent;
    std::deque<int> flashes;
    int frame = 0;
    auto luminance
        = [&](uint32_t rgba) { return lut[0][rgba & 255] + lut[1][(rgba >> 8) & 255] + lut[2][(rgba >> 16) & 255]; };
    for (;; ++frame) {
        const size_t n = std::fread(pixels.data(), 1, count * 4, stdin);
        if (!n && std::feof(stdin))
            break;
        if (n != count * 4)
            return 2;
        auto& completed = completionFrames[frame % coalescingFrames];
        for (size_t i : completed) {
            if (!--completions[i]) {
                mask[i] = 0;
                --activePixels;
            }
        }
        completed.clear();
        for (size_t i = 0; i < count; ++i) {
            const auto rgb = pixels[i];
            if (frame && rgb == last[i])
                continue;
            const int level = luminance(rgb);
            sum += level - luminance(last[i]);
            last[i] = rgb;
            const int r = linearChannel[rgb & 255], g = linearChannel[(rgb >> 8) & 255],
                      b = linearChannel[(rgb >> 16) & 255];
            const bool saturated = r > 0 && r >= 4 * (g + b);
            const auto chromaticity = Chromaticity::fromLinear(r, g, b);
            if (!frame) {
                states[i].general.low = states[i].general.high = int16_t(level);
                states[i].red.extreme = chromaticity;
                states[i].red.redSide = saturated;
                if (saturated)
                    states[i].red.opposite = { 0, 0 };
                continue;
            }
            const bool generalFlash = states[i].general.sample(level);
            const bool redFlash = states[i].red.sample(chromaticity, saturated);
            if (generalFlash || redFlash) {
                if (!completions[i]++) {
                    mask[i] = 1;
                    ++activePixels;
                }
                completed.push_back(i);
            }
        }
        int x = 0, y = 0;
        // Count once for each coalesced event, only when new paired transitions
        // complete. Retaining a mask must not itself create additional flashes.
        if (!completed.empty() && frame - lastFlash >= coalescingFrames
            && std::any_of(fields.begin(), fields.end(),
                [&](const Field& f) { return f.qualifies(mask, width, activePixels, x, y); })) {
            lastFlash = frame;
            ++qualified;
            while (!flashes.empty() && frame - flashes.front() >= fps)
                flashes.pop_front();
            flashes.push_back(frame);
            peak = std::max(peak, int(flashes.size()));
            if (flashes.size() > 3) {
                ++hits;
                if (firstFrame < 0) {
                    firstFrame = frame;
                    firstX = x;
                    firstY = y;
                }
            }
        }
        const double mean = double(sum) / (4096. * count);
        if (recent.size() > size_t(fps / 3))
            recent.pop_front();
        for (double a : recent)
            maxSwing = std::max(maxSwing, std::abs(a - mean));
        recent.push_back(mean);
    }
    std::printf("{\"frames\":%d,\"fps\":%d,\"width\":%d,\"height\":%d,\"maxSwing\":%.8f,"
                "\"areaQualifiedFlashes\":%d,\"peakFlashesPerSecond\":%d,\"rapidExcursions\":%d,"
                "\"firstFrame\":%d,\"firstWindow\":[%d,%d]}\n",
        frame, fps, width, height, maxSwing, qualified, peak, hits, firstFrame, firstX, firstY);
    return frame >= fps && maxSwing <= 0.06 && !hits ? 0 : 1;
}
