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
#include <map>
#include <utility>
#include <vector>

// A completed opposing pair returns the frame of its first transition.
struct Pair {
  int first = -1;
  int transition(int frame) {
    if (first >= 0)
      return std::exchange(first, -1);
    first = frame;
    return -1;
  }
};

struct GeneralFlash {
  int16_t low = 0, high = 0;
  Pair pair;
  int8_t direction = 0;
  int sample(int level, int frame) {
    int next = 0;
    if (direction <= 0 && level - low >= 410 && low < 3277)
      next = 1;
    else if (direction >= 0 && high - level >= 410 && level < 3277)
      next = -1;
    if (next) {
      direction = int8_t(next);
      low = high = int16_t(level);
      return pair.transition(frame);
    }
    low = std::min(low, int16_t(level));
    high = std::max(high, int16_t(level));
    return -1;
  }
};
struct Chromaticity {
  float u, v;
  float distance2(Chromaticity b) const {
    return (u - b.u) * (u - b.u) + (v - b.v) * (v - b.v);
  }
  static Chromaticity fromLinear(float r, float g, float b) {
    const float x = .4124564f * r + .3575761f * g + .1804375f * b;
    const float y = .2126729f * r + .7151522f * g + .0721750f * b;
    const float z = .0193339f * r + .1191920f * g + .9503041f * b;
    const float d = x + 15 * y + 3 * z;
    return d > 0 ? Chromaticity{4 * x / d, 9 * y / d} : Chromaticity{0, 0};
  }
};
// WCAG's saturated-red transition uses a distance >0.2 in CIE 1976 UCS.
// Keep a turning point on each side so a gradual red/neutral fade is counted.
struct RedFlash {
  Chromaticity extreme{}, opposite{.450704f, .522887f}; // sRGB red
  bool redSide = false;
  Pair pair;
  int sample(Chromaticity colour, bool saturated, int frame) {
    if (redSide == saturated) {
      if (colour.distance2(opposite) > extreme.distance2(opposite))
        extreme = colour;
    } else if (colour.distance2(extreme) > .2f * .2f) {
      opposite = extreme;
      extreme = colour;
      redSide = saturated;
      return pair.transition(frame);
    }
    return -1;
  }
};
struct Pixel {
  GeneralFlash general;
  RedFlash red;
  int generalEnd = -1, redEnd = -1;

  // Match one completed pair from each detector when their transition
  // intervals overlap. Keep the match beyond the area-coalescing interval:
  // a gradual fade can cross the two thresholds several frames apart.
  // Consume both ends on a match so one long pair cannot hide several flashes.
  int completion(int generalStart, int redStart, int frame) {
    if (generalStart >= 0 && redStart >= 0) {
      generalEnd = redEnd = -1;
      return std::max(generalStart, redStart);
    }
    if (generalStart < 0 && redStart < 0)
      return -1;
    const bool isGeneral = generalStart >= 0;
    const int first = isGeneral ? generalStart : redStart;
    int &ownEnd = isGeneral ? generalEnd : redEnd;
    int &otherEnd = isGeneral ? redEnd : generalEnd;
    if (first < otherEnd) {
      ownEnd = otherEnd = -1;
      return -1;
    }
    ownEnd = frame;
    return first;
  }
};

struct Window {
  int lastQualified = -1000, lastFlash = -1000;
  uint8_t frequency = 0;
};
struct Completion {
  uint32_t pixel;
  int first;
};
struct Exposure {
  std::vector<bool> pixels;
  std::vector<int> origins;
};
struct Field {
  int x, y, width, height, windowWidth, windowHeight;
  std::vector<Window> windows;
  std::vector<Exposure> second;
  std::vector<int> sums;

  Field(int left, int top, int visibleWidth, int visibleHeight,
        int physicalWidth, int physicalHeight)
      : x(left), y(top), width(visibleWidth), height(visibleHeight),
        windowWidth(physicalWidth), windowHeight(physicalHeight) {}

  // An integral image gives the exact concurrent area at every sliding
  // origin. A sustained exposure can recur if the same pixels flash again
  // or if both legs of another flash follow the previous observation.
  void integrate(const std::vector<uint8_t> &mask, int stride,
                 const std::vector<int> &pairStarts,
                 const std::vector<bool> *previous = nullptr, int prior = -1) {
    const int pitch = width + 1;
    sums.resize(size_t(pitch) * (height + 1));
    for (int row = 0; row < height; ++row) {
      const auto *input = mask.data() + size_t(y + row) * stride + x;
      const auto *starts = pairStarts.data() + size_t(y + row) * stride + x;
      auto *above = sums.data() + size_t(row) * pitch;
      auto *next = above + pitch;
      int sum = 0;
      for (int col = 0; col < width; ++col) {
        sum += input[col] && (!previous || starts[col] > prior ||
                              (*previous)[size_t(row) * width + col]);
        next[col + 1] = above[col + 1] + sum;
      }
    }
  }
  int area(int col, int row, int w, int h) const {
    const int pitch = width + 1;
    const auto *top = sums.data() + size_t(row) * pitch + col;
    const auto *bottom = top + size_t(h) * pitch;
    return bottom[w] - bottom[0] - top[w] + top[0];
  }
  int observe(const std::vector<uint8_t> &mask, int stride, int changes,
              const std::vector<int> &pairStarts, int frame, int fps,
              int interval, bool fresh, int &left, int &top) {
    if (second.empty())
      second.resize(fps);
    auto &current = second[frame % fps];
    for (int i : current.origins)
      --windows[i].frequency;
    current.origins.clear();
    const int limit = windowWidth * windowHeight / 4;
    if (changes <= limit)
      return 0;
    // An encoded seam may contain only part of the physical 10-degree
    // window. Its threshold still uses the full physical window area.
    const int w = std::min(windowWidth, width),
              h = std::min(windowHeight, height);
    const int columns = width - w + 1, rows = height - h + 1;
    if (windows.empty())
      windows.resize(size_t(columns) * rows);
    int peak = 0;
    const auto record = [&](int i) {
      auto &win = windows[i];
      win.lastFlash = frame;
      current.origins.push_back(i);
      if (++win.frequency > peak) {
        peak = win.frequency;
        left = x + i % columns;
        top = y + i / columns;
      }
    };
    std::map<int, std::vector<int>> sustained;
    integrate(mask, stride, pairStarts);
    for (int row = 0; row < rows; ++row) {
      for (int col = 0; col < columns; ++col) {
        if (area(col, row, w, h) <= limit)
          continue;
        const int i = row * columns + col;
        auto &win = windows[i];
        const bool onset = win.lastQualified != frame - 1;
        win.lastQualified = frame;
        if (!fresh || frame - win.lastFlash < interval)
          continue;
        if (onset || frame - win.lastFlash >= fps)
          record(i);
        else
          sustained[win.lastFlash].push_back(i);
      }
    }
    for (const auto &[prior, origins] : sustained) {
      integrate(mask, stride, pairStarts, &second[prior % fps].pixels, prior);
      for (int i : origins)
        if (area(i % columns, i / columns, w, h) > limit)
          record(i);
    }
    if (!current.origins.empty()) {
      current.pixels.resize(size_t(width) * height);
      for (int row = 0; row < height; ++row) {
        const auto *first = mask.data() + size_t(y + row) * stride + x;
        std::copy_n(first, width, current.pixels.begin() + size_t(row) * width);
      }
    }
    return peak;
  }
};

int main(int argc, char **argv) {
  if (argc < 4 || (argc - 4) % 6 != 0)
    return 2;
  const int width = std::atoi(argv[1]), height = std::atoi(argv[2]),
            fps = std::atoi(argv[3]);
  if (width < 1 || height < 1 || width > 16384 || height > 16384 || fps < 6 ||
      fps > 240)
    return 2;
  std::vector<Field> fields;
  for (int i = 4; i < argc; i += 6) {
    Field f{std::atoi(argv[i]),     std::atoi(argv[i + 1]),
            std::atoi(argv[i + 2]), std::atoi(argv[i + 3]),
            std::atoi(argv[i + 4]), std::atoi(argv[i + 5])};
    if (f.x < 0 || f.y < 0 || f.width < 1 || f.height < 1 || f.width > width ||
        f.height > height || f.x > width - f.width || f.y > height - f.height ||
        f.windowWidth < 1 || f.windowHeight < 1 || f.windowWidth > 16384 ||
        f.windowHeight > 16384)
      return 2;
    fields.push_back(f);
  }
  if (fields.empty())
    fields.push_back({0, 0, width, height,
                      std::max(1, int(std::lround(width * 341. / 1024))),
                      std::max(1, int(std::lround(height * 256. / 768)))});
  const size_t count = size_t(width) * height;
  std::array<std::array<int, 256>, 3> lut{};
  std::array<int, 256> linearChannel{};
  constexpr std::array weights{0.2126, 0.7152, 0.0722};
  for (int i = 0; i < 256; ++i) {
    const double s = i / 255.;
    const double linear =
        s <= 0.04045 ? s / 12.92 : std::pow((s + 0.055) / 1.055, 2.4);
    linearChannel[i] = int(std::round(4096 * linear));
    for (int c = 0; c < 3; ++c)
      lut[c][i] = int(std::round(4096 * weights[c] * linear));
  }
  std::vector<uint32_t> pixels(count), last(count);
  std::vector<Pixel> states(count);
  std::vector<int> pairStarts(count, -1);
  constexpr int coalescingFrames = 3;
  std::vector<uint8_t> mask(count), completions(count);
  std::array<std::vector<Completion>, coalescingFrames> completionFrames;
  int activePixels = 0, lastDiagnosticFlash = -coalescingFrames;
  int hits = 0, qualified = 0, peak = 0;
  int firstFrame = -1, firstX = -1, firstY = -1;
  double maxSwing = 0.;
  uint64_t sum = 0;
  std::deque<double> recent;
  int frame = 0;
  auto luminance = [&](uint32_t rgba) {
    return lut[0][rgba & 255] + lut[1][(rgba >> 8) & 255] +
           lut[2][(rgba >> 16) & 255];
  };
  for (;; ++frame) {
    const size_t n = std::fread(pixels.data(), 1, count * 4, stdin);
    if (!n && std::feof(stdin))
      break;
    if (n != count * 4)
      return 2;
    auto &completed = completionFrames[frame % coalescingFrames];
    for (const auto &old : completed) {
      pairStarts[old.pixel] = -1;
      if (!--completions[old.pixel]) {
        mask[old.pixel] = 0;
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
      const int r = linearChannel[rgb & 255],
                g = linearChannel[(rgb >> 8) & 255],
                b = linearChannel[(rgb >> 16) & 255];
      const bool saturated = r > 0 && r >= 4 * (g + b);
      const auto chromaticity = Chromaticity::fromLinear(r, g, b);
      if (!frame) {
        states[i].general.low = states[i].general.high = int16_t(level);
        states[i].red.extreme = chromaticity;
        states[i].red.redSide = saturated;
        if (saturated)
          states[i].red.opposite = {0, 0};
        continue;
      }
      const int generalStart = states[i].general.sample(level, frame);
      const int redStart = states[i].red.sample(chromaticity, saturated, frame);
      const int first = states[i].completion(generalStart, redStart, frame);
      if (first >= 0) {
        if (!completions[i]++) {
          mask[i] = 1;
          ++activePixels;
        }
        completed.push_back({uint32_t(i), first});
      }
    }
    // Only evidence whose completion is still in the coalescing interval
    // contributes a pair start to the concurrent-area mask.
    for (const auto &group : completionFrames) {
      for (const auto &pair : group)
        pairStarts[pair.pixel] = std::max(pairStarts[pair.pixel], pair.first);
    }
    int framePeak = 0;
    for (auto &field : fields) {
      int x = 0, y = 0;
      const int frequency = field.observe(mask, width, activePixels, pairStarts,
                                          frame, fps, coalescingFrames,
                                          !completed.empty(), x, y);
      framePeak = std::max(framePeak, frequency);
      if (frequency > 3 && firstFrame < 0) {
        firstFrame = frame;
        firstX = x;
        firstY = y;
      }
    }
    // Only the diagnostic event total is shared across windows. Frequency
    // and the pass/fail decision belong to a fixed visual-field origin.
    if (framePeak && frame - lastDiagnosticFlash >= coalescingFrames) {
      ++qualified;
      lastDiagnosticFlash = frame;
    }
    peak = std::max(peak, framePeak);
    hits += framePeak > 3;
    const double mean = double(sum) / (4096. * count);
    if (recent.size() > size_t(fps / 3))
      recent.pop_front();
    for (double a : recent)
      maxSwing = std::max(maxSwing, std::abs(a - mean));
    recent.push_back(mean);
  }
  std::printf(
      "{\"frames\":%d,\"fps\":%d,\"width\":%d,\"height\":%d,\"maxSwing\":%.8f,"
      "\"areaQualifiedFlashes\":%d,\"peakFlashesPerSecond\":%d,"
      "\"rapidExcursions\":%d,"
      "\"firstFrame\":%d,\"firstWindow\":[%d,%d]}\n",
      frame, fps, width, height, maxSwing, qualified, peak, hits, firstFrame,
      firstX, firstY);
  return frame >= fps && maxSwing <= 0.06 && !hits ? 0 : 1;
}
