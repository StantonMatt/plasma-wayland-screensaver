// SPDX-License-Identifier: GPL-3.0-or-later
#include "visuals_depth_ribbons.h"
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QDebug>
#include <cmath>
#include <vector>
#include <algorithm>

extern "C" void visuals_ribbons_capture_unflapped();

namespace {
QJsonObject captureRequest()
{
    QFile file(qEnvironmentVariable("PVS_VISUAL_CAPTURE_SCRIPT"));
    return !file.fileName().isEmpty() && file.open(QIODevice::ReadOnly)
        ? QJsonDocument::fromJson(file.readAll()).object() : QJsonObject{};
}

void configureFlagControl()
{
    if (captureRequest().value(QStringLiteral("unflappedControl")).toBool())
        visuals_ribbons_capture_unflapped();
}
Q_CONSTRUCTOR_FUNCTION(configureFlagControl)

#define RIBBON_KNOT(x0, y0, nx0, ny0, w0, f0, p0, a0, x1, y1, nx1, ny1, w1, f1, p1, a1, tag) \
    {{{x0, y0}, {nx0, ny0}, w0, f0, p0, a0}, {{x1, y1}, {nx1, ny1}, w1, f1, p1, a1}, tag},
static constexpr vd_ribbon_pair knotNodes[] = {
#include "a60-knots.inc"
};
#undef RIBBON_KNOT
static constexpr vd_ribbon_draw knotDraws[] = {
    {0, 33, 0, 0, 1.f, 10, {{0.23921569f, 0.8392157f, 0.9098039f, 0.9f}, {0.22745098f, 0.5254902f, 1.0f, 1.0f}, {0.0f, 0.70146614f, 0.7919218f, 0.35f}, {0.8509804f, 0.9843137f, 1.0f, 1.0f}}},
};
#include "g600-knots.inc"

template<size_t N, size_t D>
size_t replayWitness(const vd_ribbon_pair (&pairs)[N], const vd_ribbon_draw (&ranges)[D],
    vd_ribbon_pair* nodes, size_t capacity, vd_ribbon_draw* draws, size_t drawCapacity, size_t* nodeCount)
{
    if (N > capacity || D > drawCapacity)
        qFatal("Insufficient room for the Ribbons geometry fixture");
    std::copy_n(pairs, N, nodes);
    std::copy_n(ranges, D, draws);
    *nodeCount = N;
    return D;
}
// Place the isolated native head and its face mask side by side. Both copies
// retain the original orientation and subpixel registration. Keep every fitted
// head section and enough of the shoulder to cover the forward measurement disk.
size_t isolateHead(int ordinal, const visuals_depth_sample* sample, vd_ribbon_pair* nodes,
    vd_ribbon_draw* draws, size_t count, size_t* nodeCount)
{
    vd_sample_info info {};
    if (visuals_depth_sample_info(sample, &info))
        qFatal("Missing native head sample");
    const float t = float(info.alpha);
    const auto at = [t](float a, float b) { return std::lerp(a, b, t); };
    const double width = info.view.viewport.width, height = info.view.viewport.height;
    int visible = 0;
    vd_ribbon_draw selected {};
    for (size_t i = 0; i < count; ++i) {
        const auto& draw = draws[i];
        if (draw.motes || draw.material < 6 || draw.count < 3 || nodes[draw.first].lower.cap_mix >= 0)
            continue;
        const auto& tip = nodes[draw.first + 1];
        const double x = at(tip.lower.position[0], tip.upper.position[0]);
        const double y = at(tip.lower.position[1], tip.upper.position[1]);
        if (x >= 0 && x < width && y >= 0 && y < height) {
            if (visible == ordinal)
                selected = draw;
            ++visible;
        }
    }
    QJsonObject meta {{QStringLiteral("ns"), QString::number(info.view.timeline_ns)},
        {QStringLiteral("visible"), visible}, {QStringLiteral("ordinal"), ordinal}};
    if (selected.count) {
        const auto* src = nodes + selected.first;
        const auto coord = [&](unsigned i, unsigned k) {
            return at(src[i].lower.position[k], src[i].upper.position[k]);
        };
        const float x = coord(1, 0), y = coord(1, 1);
        const float fx = coord(0, 0) - x, fy = coord(0, 1) - y;
        const float distance = std::hypot(fx, fy);
        const float radius = at(src[1].lower.halo_radius, src[1].upper.halo_radius);
        // Include the two shoulder flanks and the fixed 160 px forward probe.
        constexpr unsigned shoulder = 9;
        const auto shoulderIndex = std::min(shoulder, selected.count - 1);
        const float shoulderDistance = std::hypot(coord(shoulderIndex, 0) - x, coord(shoulderIndex, 1) - y);
        const float shoulderWidth = std::max(src[shoulderIndex].lower.half_width, src[shoulderIndex].upper.half_width);
        const float probeRadius = std::max(1.2f * radius, shoulderDistance + shoulderWidth + 160.f);
        const int margin = int(std::ceil(probeRadius)) + 10;
        const float dx = std::round(float(margin) - x);
        const float dy = std::round(float(height * 0.5) - y);
        const int separation = int(width) - 2 * margin;
        meta.insert(QStringLiteral("maskOffset"), separation);
        meta.insert(QStringLiteral("x"), x + dx);
        meta.insert(QStringLiteral("y"), y + dy);
        meta.insert(QStringLiteral("sourceX"), x);
        meta.insert(QStringLiteral("sourceY"), y);
        meta.insert(QStringLiteral("fx"), fx / distance);
        meta.insert(QStringLiteral("fy"), fy / distance);
        meta.insert(QStringLiteral("radius"), radius);
        meta.insert(QStringLiteral("layer"), int(selected.material / 6));
        unsigned n = 2;
        while (n < selected.count && (n <= shoulder || std::hypot(coord(n-1, 0)-x, coord(n-1, 1)-y) <= 1.7f * radius))
            ++n;
        // Retain the two native support brackets for diagnosing a clipped
        // cap. Normals carry the curvature budget used by the vertex shader.
        QJsonArray sections;
        for (unsigned j = 0; j < n; ++j) {
            QJsonArray section;
            for (const auto* end : {&src[j].lower, &src[j].upper})
                section.append(QJsonArray{end->position[0], end->position[1],
                    end->normal[0], end->normal[1], end->half_width, end->facing,
                    end->halo_radius, end->cap_mix, end->tip_distance});
            sections.append(section);
        }
        meta.insert(QStringLiteral("sections"), sections);
        meta.insert(QStringLiteral("fraction"), t);
        // Include every source span whose emitted light can reach the disk,
        // even when a distant part of this strand turns back toward its head.
        // Excluding disjoint spans prevents the two translated copies from
        // drawing over each other's measurement region.
        n = selected.count;
        std::vector<bool> include(n, false);
        for (unsigned j = 0; j+1 < n; ++j) {
            const float ax = coord(j, 0), ay = coord(j, 1);
            const float vx = coord(j+1, 0)-ax, vy = coord(j+1, 1)-ay;
            const float chord = std::hypot(vx, vy);
            float reach = 0.f;
            for (const auto* k : {&src[j].lower, &src[j].upper, &src[j+1].lower, &src[j+1].upper}) {
                const float face = 1.27f * k->half_width;
                const float halo = std::max(7.f * selected.silk.halo[3] * k->half_width,
                    1.2f * k->halo_radius);
                reach = std::max(reach, face+halo);
            }
            const float bound = reach + chord/3.f + probeRadius + 6.f;
            const auto touches = [&](float px) {
                const float t = std::clamp(((px-ax)*vx+(y-ay)*vy)/std::max(chord*chord,0.00001f),0.f,1.f);
                return std::hypot(ax+t*vx-px,ay+t*vy-y) <= bound;
            };
            include[j] = touches(x);
            if (include[j] && (touches(x+separation) || touches(x-separation)))
                qFatal("Overlapping diagnostic panels for a single-strand span");
        }
        std::vector<vd_ribbon_pair> local(src, src+n);
        const auto material = selected.material;
        for (unsigned copy=0; copy<2; ++copy) {
            auto range = selected;
            range.first = copy*n;
            range.strand = copy;
            range.count = n;
            range.material = material / 6 * 6 + (material + copy) % 6;
            if (copy) {
                // Black halo RGB retains the complete production geometry.
                std::fill_n(range.silk.halo, 3, 0.f);
                for (auto rgb : {range.silk.front, range.silk.back, range.silk.accent})
                    std::fill_n(rgb, 3, 1.f);
            }
            draws[copy] = range;
            for (unsigned j=0; j<n; ++j) {
                auto node = local[j];
                for (auto k : {&node.lower, &node.upper}) {
                    k->position[0] += dx + float(copy * separation);
                    k->position[1] += dy;
                }
                node.tag = include[j] ? float(range.material + copy*32) : 65535.f;
                nodes[range.first+j] = node;
            }
        }
        *nodeCount = 2*n;
    } else {
        *nodeCount = 0;
    }
    qInfo().noquote() << "HeadSample" << QJsonDocument(meta).toJson(QJsonDocument::Compact);
    return selected.count ? 2 : 0;
}

}

extern "C" bool __real_visuals_ribbons_same_pair(const visuals_depth_sample*, const visuals_depth_sample*);

extern "C" bool __wrap_visuals_ribbons_same_pair(const visuals_depth_sample* previous, const visuals_depth_sample* current)
{
    // Isolation selects a visible head and registers the mask at the actual
    // presentation fraction. Refresh the capture fixture on both presentations.
    static const bool isolating = captureRequest().contains(QStringLiteral("ribbonHeadOrdinal"));
    return !isolating && __real_visuals_ribbons_same_pair(previous, current);
}

extern "C" size_t __real_visuals_ribbons_write(const visuals_depth_sample*, vd_ribbon_pair*, size_t,
    vd_ribbon_draw*, size_t, size_t*);

// In the capture executable, replay the real A/60 geometry twice. The second
// fixture changes only the identity at a joint between the two crossing spans.
extern "C" size_t __wrap_visuals_ribbons_write(const visuals_depth_sample* sample, vd_ribbon_pair* nodes,
    size_t capacity, vd_ribbon_draw* draws, size_t drawCapacity, size_t* nodeCount)
{
    static const QJsonObject request = captureRequest();
    const auto witness = request.value(QStringLiteral("ribbonWitness")).toString();
    if (witness == QStringLiteral("G/600"))
        return replayWitness(fanNodes, fanDraws, nodes, capacity, draws, drawCapacity, nodeCount);

    const size_t count = witness == QStringLiteral("A/60")
        ? replayWitness(knotNodes, knotDraws, nodes, capacity, draws, drawCapacity, nodeCount)
        : __real_visuals_ribbons_write(sample, nodes, capacity, draws, drawCapacity, nodeCount);
    if (request.contains(QStringLiteral("ribbonHeadOrdinal")))
        return isolateHead(request.value(QStringLiteral("ribbonHeadOrdinal")).toInt(), sample, nodes, draws, count, nodeCount);
    const auto split = request.value(QStringLiteral("splitRibbonIdentity")).toObject();
    if (split.isEmpty() || count >= drawCapacity)
        return count;

    const auto strand = split.value(QStringLiteral("strand")).toInt(-1);
    const auto joint = split.value(QStringLiteral("joint")).toInt(-1);
    for (size_t i = 0; i < count; ++i) {
        auto& draw = draws[i];
        if (draw.motes || draw.count < 3 || int(nodes[draw.first].tag) / 32 != strand
            || joint < 1 || unsigned(joint + 1) >= draw.count)
            continue;

        // The ordinal occupies bits 5..11. Give the second strip the next
        // depth band. Keep the ordering of every subsequent ribbon and leave
        // spark and break tags intact.
        for (size_t n = draw.first + joint; n < *nodeCount; ++n) {
            if (nodes[n].tag < 4096.f)
                nodes[n].tag += 32.f;
        }
        std::copy_backward(draws + i + 1, draws + count, draws + count + 1);
        auto& second = draws[i + 1];
        second = draw;
        second.first += joint;
        second.count -= joint;
        draw.count = joint + 1;
        return count + 1;
    }
    return count;
}

extern "C" size_t __real_visuals_ribbons_write_packed(const visuals_depth_sample*,vd_ribbon_motion*,size_t,
    vd_ribbon_cap*,size_t,size_t*,vd_ribbon_draw*,size_t,size_t*);
extern "C" size_t visuals_ribbons_pack_fixture(const vd_ribbon_pair*,size_t,vd_ribbon_motion*,vd_ribbon_cap*);

extern "C" size_t __wrap_visuals_ribbons_write_packed(const visuals_depth_sample* sample,
    vd_ribbon_motion* nodes, size_t capacity, vd_ribbon_cap* caps, size_t capCapacity,
    size_t* capCount, vd_ribbon_draw* draws, size_t drawCapacity, size_t* nodeCount)
{
    static const QJsonObject request = captureRequest();
    if (!request.contains(QStringLiteral("ribbonHeadOrdinal")) && !request.contains(QStringLiteral("ribbonWitness"))
        && !request.contains(QStringLiteral("splitRibbonIdentity")))
        return __real_visuals_ribbons_write_packed(sample,nodes,capacity,caps,capCapacity,capCount,draws,drawCapacity,nodeCount);
    std::vector<vd_ribbon_pair> source(capacity);
    const auto count = __wrap_visuals_ribbons_write(sample,source.data(),capacity,draws,drawCapacity,nodeCount);
    if (*nodeCount > capCapacity)
        qFatal("Insufficient cap storage for native fixture");
    *capCount=visuals_ribbons_pack_fixture(source.data(),*nodeCount,nodes,caps);
    return count;
}
