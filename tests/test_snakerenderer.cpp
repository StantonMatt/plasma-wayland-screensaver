// SPDX-License-Identifier: GPL-3.0-or-later
#include "snakerenderer.h"
#include "configuration.h"
#include "snakematerial.h"
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QTemporaryDir>
#include <QFile>
#include <QSignalSpy>
#include <QSGGeometryNode>
#include <QSGTexture>
#include <QTest>
#include <QElapsedTimer>
#include <QCryptographicHash>
#include <numeric>
#include <cmath>
#include <limits>
#include <cstddef>
#include <rhi/qrhi.h>

class SnakeRendererTest final : public QObject
{
    Q_OBJECT
    static SnakeFrame makeFrame(int segments, int snakeCount = 1, int foodCount = 1,
                                bool malformed = false, float coordinateScale = 1)
    {
        SnakeFrame frame;
        frame.info = {600, 20, 3440, 1440, 0, 1, 0, {}, {}};
        for (int n = 0; n < snakeCount; ++n) {
            frame.snakes.push_back({uint32_t(n), 1, 1, 0, 8, 0, 0.7,
                                   uint32_t(frame.segments.size()), uint32_t(segments), SNAKES_CORE_LEADER, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
            for (int i = 0; i < segments; ++i) {
                const float x = (250 - i * 4.5 + (n % 4) * 780) * coordinateScale;
                const float y = (120 + std::sin(i * 0.08) * 18 + (n / 4) * 340) * coordinateScale;
                frame.segments.push_back({malformed && i == 7 ? std::numeric_limits<float>::quiet_NaN() : x,
                                          y, x - 1, y});
            }
        }
        for (int i = 0; i < foodCount; ++i) {
            const float x = 12 + (i % 32) * 9;
            const float y = 12 + (i / 32) * 9;
            frame.food.push_back({uint64_t(i + 1), x, y, 4, 0, 0, x, y, 0, SNAKES_CORE_FOOD_SPARK, 0, 0, 0, 0, 0, 0, 0, 0});
        }
        return frame;
    }
    static SnakeFrame anatomyFrame()
    {
        constexpr double W = 1280, H = 720, r = 0.0108 * H;
        constexpr double spacing = 1.18 * r;
        const double rows[] = {0.22, 0.335, 0.45, 0.565, 0.68};
        const int lengths[] = {16, 48, 110, 260, 280};
        const uint32_t colors[] = {1, 2, 3, 4, 0};
        const uint32_t flags[] = {0, SNAKES_CORE_BOOSTING, SNAKES_CORE_HUNTING, 0, SNAKES_CORE_LEADER};
        SnakeFrame frame;
        frame.info = {600, 20, W, H, 0, 1, 0, {}, {}};
        for (int k = 0; k < 5; ++k) {
            const auto path = [=](double x) { return rows[k] * H + 3.6 * r * std::sin(2 * M_PI * x / (0.2 * W) + 1.3 * k); };
            const auto arc = [=](double x, double dx) {
                const auto speed = [=](double x) {
                    const double slope = 3.6 * r * (2 * M_PI / (0.2 * W)) * std::cos(2 * M_PI * x / (0.2 * W) + 1.3 * k);
                    return std::sqrt(1 + slope * slope);
                };
                return dx / 6 * (speed(x) + 4 * speed(x - dx / 2) + speed(x - dx));
            };
            const auto offset = uint32_t(frame.segments.size());
            double x = W * (0.35 + 0.12 * k);
            for (int j = 0; j < lengths[k]; ++j) {
                const float y = path(x);
                frame.segments.push_back({float(x), y, float(x), y});
                double low = 0, high = spacing;
                for (int b = 0; b < 24; ++b) {
                    const double dx = (low + high) / 2;
                    if (arc(x, dx) < spacing) low = dx; else high = dx;
                }
                x -= (low + high) / 2;
            }
            const auto &a = frame.segments[offset], &b = frame.segments[offset + 1];
            const double angle = std::atan2(a.y - b.y, a.x - b.x);
            frame.snakes.push_back({uint32_t(k), 1, 1, colors[k], r, angle, angle,
                                   offset, uint32_t(lengths[k]), flags[k], 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
        }
        // A visible boost tail and its retained history, separate from the long
        // Anatomy adult whose tail position depends on the sine path.
        const auto boostOffset = uint32_t(frame.segments.size());
        for (int j = 0; j < 24; ++j) {
            const float x = 1070 - j * spacing;
            frame.segments.push_back({x, 75, x, 75});
        }
        frame.snakes.push_back({5, 1, 1, 2, r, 0, 0, boostOffset, 24, SNAKES_CORE_BOOSTING, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
        // A tight spiral starts at 1.8r, then opens with 4r between turns.
        // Arc spacing matches the body; wider outer turns expose the inner
        // glow fold without piling multiple opaque laps on top of that wedge.
        const auto coilOffset = uint32_t(frame.segments.size());
        double theta = 0;
        for (int j = 0; j < 48; ++j) {
            const double radius = r * (1.8 + 0.65 * theta);
            const float x = 1135 + radius * std::cos(theta), y = 320 + radius * std::sin(theta);
            frame.segments.push_back({x, y, x, y});
            const auto speed = [=](double t) { return r * std::hypot(1.8 + 0.65 * t, 0.65); };
            double low = 0, high = spacing / radius;
            for (int b = 0; b < 24; ++b) {
                const double dt = (low + high) / 2;
                const double arc = dt / 6 * (speed(theta) + 4 * speed(theta + dt / 2) + speed(theta + dt));
                if (arc < spacing) low = dt; else high = dt;
            }
            theta += (low + high) / 2;
        }
        const auto &a = frame.segments[coilOffset], &b = frame.segments[coilOffset + 1];
        const double angle = std::atan2(a.y - b.y, a.x - b.x);
        frame.snakes.push_back({6, 1, 1, 4, r, angle, angle, coilOffset, 48, SNAKES_CORE_HUNTING, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
        // World::add_food exports R*(0.23 + min(value,1.4)*0.13).
        // Representative values: spark 0.7, shard 0.85, pellet 0.5. Prism
        // spawning belongs to R4; reserve its approved 0.62R size here.
        const float sizes[] = {float(r * 0.321), float(r * 0.3405), float(r * 0.295), float(r * 0.62)};
        for (int group = 0; group < 5; ++group) {
            const uint8_t kind = group == 4 ? SNAKES_CORE_FOOD_SPARK : group;
            for (int j = 0; j < (kind == 3 ? 1 : 3); ++j) {
                const float x = W * (0.06 + group * 0.075) + (j - 1) * r * 2.6;
                const float y = H * 0.86 - r * 0.5 + (j % 2) * r * 1.6;
                frame.food.push_back({uint64_t(frame.food.size() + 1), x, y, sizes[kind], float(j * 1.7),
                    float(group == 4 && j == 2 ? 0.8 : 0.0), x + 40, y, uint32_t((group + j) % 6),
                    kind, uint8_t(group == 4 ? 12 : 255), 0, 0, 0, 0, 0, 0, 0});
            }
        }
        return frame;
    }
    static SnakeFrame powerupsFrame()
    {
        SnakeFrame frame;
        frame.info = {600, 20, 1280, 720, 0, 1, 0, {}, {}};
        constexpr double r = 9, spacing = 1.18 * r;
        for (uint8_t kind = 1; kind <= 3; ++kind) {
            const double x = 215 + (kind - 1) * 425;
            const auto offset = uint32_t(frame.segments.size());
            for (int j = 0; j < 42; ++j) {
                const float px = x + 2 * r * std::sin(j * 0.18);
                const float py = 195 + j * spacing;
                frame.segments.push_back({px, py, px, py});
            }
            const auto &a = frame.segments[offset], &b = frame.segments[offset + 1];
            const auto angle = std::atan2(a.y-b.y, a.x-b.x);
            snakes_core_snake snake{};
            snake.id = kind - 1; snake.generation = 1; snake.alive = 1;
            snake.color_index = kind; snake.radius = r; snake.angle = angle;
            snake.desired_angle = angle; snake.segment_offset = offset; snake.segment_count = 42;
            snake.effect_kind = kind; snake.effect_ticks = kind == 1 ? 24 : kind == 2 ? 180 : 90;
            snake.flags = kind == 1 ? SNAKES_CORE_BOOSTING : kind == 3 ? SNAKES_CORE_PHASED : 0;
            frame.snakes.push_back(snake);
            snakes_core_item item{};
            item.id = kind; item.kind = kind; item.x = x; item.y = 75;
            item.age_ticks = 15; item.life_ticks = 750; frame.itemRadius = 18;
            frame.items.push_back(item);
        }
        const auto &magnet = frame.segments[frame.snakes[1].segment_offset];
        for (int k = 0; k < 12; ++k) {
            const double a = k * M_PI / 6;
            const float x = magnet.x + float(std::cos(a) * 65);
            const float y = magnet.y + float(std::sin(a) * 65);
            frame.food.push_back({uint64_t(k+1), x, y, 3, float(k), 0.85f,
                                  magnet.x, magnet.y, uint32_t(k%6), SNAKES_CORE_FOOD_SPARK, 255, 0, 0, 0, 0, 0, 0, 0});
        }
        // The Phase lane crosses a tangible rival, exposing its translucent body.
        const auto offset = uint32_t(frame.segments.size());
        for (int j = 0; j < 22; ++j) {
            const float x = 1175 - j * spacing;
            frame.segments.push_back({x, 390, x, 390});
        }
        frame.snakes.push_back({3, 1, 1, 0, r, 0, 0, offset, 22, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
        return frame;
    }
    const QVector<QColor> palette{QColor("#4de6ff")};
    static QShader bakedShader(const char *path)
    {
        // Initializes the static library's shared resource pack.
        SnakeMaterial::shadersAvailable();
        QFile file(QString::fromLatin1(path));
        if (!file.open(QIODevice::ReadOnly)) return {};
        return QShader::fromSerialized(file.readAll());
    }
    static QShader vertexShader() { return bakedShader(":/snakes/shaders/snake.vert.qsb"); }
    static QShader fragmentShader() { return bakedShader(":/snakes/shaders/snake.frag.qsb"); }
    static QByteArray geometryBytes(const QSGGeometryNode *node)
    {
        const auto *geometry = node->geometry();
        return QByteArray(static_cast<const char *>(geometry->vertexData()),
                          geometry->vertexCount() * geometry->sizeOfVertex());
    }
    static SnakeFrame animationFrame(const QString &scene)
    {
        auto frame = makeFrame(scene == QStringLiteral("head-and-body") ? 260 : 24, 1, 0);
        auto &snake = frame.snakes.front();
        snake.flags = 0;
        if (scene == QStringLiteral("food-pulse")) {
            frame = makeFrame(0, 0, 1);
        } else if (scene.startsWith(QStringLiteral("capsule-"))) {
            frame.snakes.clear(); frame.segments.clear();
            snakes_core_item item{};
            item.id = 1; item.kind = scene.right(1).toUInt();
            item.x = 250; item.y = 120; item.age_ticks = 2; item.life_ticks = 30;
            frame.items.push_back(item); frame.itemRadius = 18;
        } else if (scene == QStringLiteral("surge") || scene == QStringLiteral("contrail")) {
            snake.flags = SNAKES_CORE_BOOSTING;
            snake.effect_kind = 1; snake.effect_ticks = 24;
        } else if (scene == QStringLiteral("magnet")) {
            snake.effect_kind = 2; snake.effect_ticks = 24;
        } else if (scene == QStringLiteral("phase")) {
            snake.flags = SNAKES_CORE_PHASED;
            snake.effect_kind = 3; snake.effect_ticks = 24;
        } else if (scene == QStringLiteral("crown")) {
            snake.flags = SNAKES_CORE_LEADER;
        } else if (scene == QStringLiteral("head-and-body")) {
            // Tier-three breathing/spine lights, trapped shimmer/pupils,
            // plus the independent blink and tongue uniform clock.
            snake.flags = SNAKES_CORE_TRAPPED | SNAKES_CORE_HUNTING;
        } else if (scene == QStringLiteral("corpse")) {
            snake.flags = SNAKES_CORE_CORPSE; snake.alive = 0;
        } else {
            const uint8_t kind = scene == QStringLiteral("pickup-ring") ? SNAKES_CORE_EVENT_PICKUP
                : scene == QStringLiteral("expiry-ring") ? SNAKES_CORE_EVENT_ITEM_EXPIRY
                : scene == QStringLiteral("effect-expiry-ring") ? SNAKES_CORE_EVENT_EFFECT_EXPIRY
                : scene == QStringLiteral("succession-ring") ? SNAKES_CORE_EVENT_SUCCESSION
                : SNAKES_CORE_EVENT_KILL;
            frame.events.push_back({frame.info.tick, 250, 120, 0,
                                   kind == SNAKES_CORE_EVENT_PICKUP ? 1u : 0u, 0, kind, {}, 0, 0, 0, 0, 0, 0});
        }
        return frame;
    }
private Q_SLOTS:
    void bakedShaderResourceLayout()
    {
        const auto vertex = vertexShader(), fragment = fragmentShader();
        using Uniforms = SnakeMaterial::UniformData;
        const QList<QPair<QByteArray, int>> offsets{
            {"matrix", offsetof(Uniforms, matrix)}, {"opacity", offsetof(Uniforms, opacity)},
            {"time", offsetof(Uniforms, time)}, {"light", offsetof(Uniforms, light)},
            {"animationTime", offsetof(Uniforms, animationTime)},
            {"motionScale", offsetof(Uniforms, motionScale)}, {"paletteMode", offsetof(Uniforms, paletteMode)}};
        for (const auto &shader : {vertex, fragment}) {
            const auto description = shader.description();
            QVERIFY(description.isValid());
            const auto blocks = description.uniformBlocks();
            QCOMPARE(blocks.size(), 1);
            const auto &block = blocks.first();
            QCOMPARE(block.binding, SnakeMaterial::UniformBinding);
            QCOMPARE(block.descriptorSet, 0);
            QCOMPARE(block.size, int(sizeof(Uniforms)));
            QVERIFY(SnakeMaterial::UniformBufferSize >= block.size);
            QCOMPARE(block.members.size(), offsets.size());
            for (qsizetype i = 0; i < offsets.size(); ++i) {
                QCOMPARE(block.members[i].name, offsets[i].first);
                QCOMPARE(block.members[i].offset, offsets[i].second);
            }
            QVERIFY(description.separateImages().isEmpty());
            QVERIFY(description.separateSamplers().isEmpty());
            QVERIFY(description.storageImages().isEmpty());
            QVERIFY(description.storageBlocks().isEmpty());
        }
        // Wave origins are decoded before interpolation and cross the stages
        // as premultiplied RGB, without changing the 24-byte vertex ABI.
        const auto outputs = vertex.description().outputVariables();
        const auto inputs = fragment.description().inputVariables();
        const auto light = std::find_if(outputs.cbegin(), outputs.cend(), [](const auto &v) {
            return v.name == "waveLight";
        });
        QVERIFY(light != outputs.cend());
        QCOMPARE(light->location, 5);
        QCOMPARE(light->type, QShaderDescription::Vec3);
        QVERIFY(std::any_of(inputs.cbegin(), inputs.cend(), [&](const auto &v) {
            return v.name == light->name && v.location == light->location && v.type == light->type;
        }));
        QVERIFY(vertex.description().combinedImageSamplers().isEmpty());
        const auto samplers = fragment.description().combinedImageSamplers();
        QCOMPARE(samplers.size(), 1);
        QCOMPARE(samplers.first().name, QByteArray("iconAtlas"));
        QCOMPARE(samplers.first().type, QShaderDescription::Sampler2D);
        QCOMPARE(samplers.first().binding, SnakeMaterial::IconAtlasBinding);
        QCOMPARE(samplers.first().descriptorSet, 0);
        SnakeMaterial material;
        QCOMPARE(material.iconAtlas()->textureSize(), QSize(128, 128));
        QCOMPARE(material.iconAtlas()->filtering(), QSGTexture::Linear);
        QCOMPARE(material.iconAtlas()->horizontalWrapMode(), QSGTexture::ClampToEdge);
        QCOMPARE(material.iconAtlas()->verticalWrapMode(), QSGTexture::ClampToEdge);
        QVERIFY(!material.iconAtlas()->hasMipmaps());
    }

    void bakedShaderVariants()
    {
        const auto vertex = vertexShader(), fragment = fragmentShader();
        QVERIFY(vertex.isValid()); QVERIFY(fragment.isValid());
        QCOMPARE(vertex.stage(), QShader::VertexStage);
        QCOMPARE(fragment.stage(), QShader::FragmentStage);
        QList<QShaderKey> required;
        for (int version : {100, 300, 310, 320})
            required.append({QShader::GlslShader, {version, QShaderVersion::GlslEs}});
        for (int version : {120, 130, 140, 150, 330})
            required.append({QShader::GlslShader, version});
        required.append({QShader::SpirvShader, 100});
        required.append({QShader::HlslShader, 50});
        required.append({QShader::MslShader, 12});
        for (auto key : required) {
            QVERIFY(fragment.availableShaders().contains(key));
            QVERIFY(!fragment.shader(key).shader().isEmpty());
            for (auto variant : {QShader::StandardShader, QShader::BatchableVertexShader}) {
                key.setSourceVariant(variant);
                QVERIFY(vertex.availableShaders().contains(key));
                QVERIFY(!vertex.shader(key).shader().isEmpty());
            }
        }
        const auto inputs = vertex.description().inputVariables();
        QVERIFY(std::any_of(inputs.cbegin(), inputs.cend(), [](const auto &input) {
            return input.name == "_qt_order" && input.location == 7;
        }));
        qInfo() << "Baked shader variants:" << vertex.availableShaders().size() << "vertex,"
                << fragment.availableShaders().size() << "fragment";
    }

    void backendShaderSelection()
    {
        const auto vertex = vertexShader(), fragment = fragmentShader();
        for (auto api : {QSGRendererInterface::Vulkan, QSGRendererInterface::Direct3D11,
                        QSGRendererInterface::Direct3D12, QSGRendererInterface::Metal})
            QVERIFY(SnakeMaterial::shadersSupported(vertex, fragment, api));
        for (auto api : {QSGRendererInterface::Software, QSGRendererInterface::Unknown,
                        QSGRendererInterface::Null, QSGRendererInterface::OpenVG})
            QVERIFY(!SnakeMaterial::shadersSupported(vertex, fragment, api));
        QSurfaceFormat format;
        format.setRenderableType(QSurfaceFormat::OpenGLES);
        for (const auto version : {std::pair{2, 0}, {3, 0}, {3, 1}, {3, 2}}) {
            format.setVersion(version.first, version.second);
            QVERIFY(SnakeMaterial::shadersSupported(vertex, fragment, QSGRendererInterface::OpenGL, format));
        }
        format.setRenderableType(QSurfaceFormat::OpenGL);
        for (const auto version : {std::pair{2, 1}, {3, 0}, {3, 1}, {3, 2}, {3, 3}, {4, 6}}) {
            format.setVersion(version.first, version.second);
            format.setProfile(version.first >= 3 ? QSurfaceFormat::CoreProfile : QSurfaceFormat::NoProfile);
            QVERIFY(SnakeMaterial::shadersSupported(vertex, fragment, QSGRendererInterface::OpenGL, format));
        }
        QVERIFY(!SnakeMaterial::shadersSupported({}, fragment, QSGRendererInterface::Vulkan));
        QVERIFY(!SnakeMaterial::shadersSupported(vertex, {}, QSGRendererInterface::Vulkan));
        QVERIFY(!SnakeMaterial::shadersSupported(fragment, vertex, QSGRendererInterface::Vulkan));
        for (auto variant : {QShader::StandardShader, QShader::BatchableVertexShader}) {
            auto missing = vertex;
            missing.removeShader({QShader::SpirvShader, 100, variant});
            QVERIFY(!SnakeMaterial::shadersSupported(missing, fragment, QSGRendererInterface::Vulkan));
        }
        auto missingFragment = fragment;
        missingFragment.removeShader({QShader::SpirvShader, 100});
        QVERIFY(!SnakeMaterial::shadersSupported(vertex, missingFragment, QSGRendererInterface::Vulkan));
        // GLES cannot consume desktop GLSL even when both shaders are valid.
        auto desktopOnly = vertex;
        for (auto key : desktopOnly.availableShaders())
            if (key.source() == QShader::GlslShader && key.sourceVersion().flags().testFlag(QShaderVersion::GlslEs))
                desktopOnly.removeShader(key);
        format.setRenderableType(QSurfaceFormat::OpenGLES);
        format.setVersion(3, 2);
        QVERIFY(!SnakeMaterial::shadersSupported(desktopOnly, fragment, QSGRendererInterface::OpenGL, format));
        // ES 3.2-only packs cannot be chosen by an ES 3.0 context.
        auto es32Only = vertex;
        for (auto key : es32Only.availableShaders())
            if (key.source() == QShader::GlslShader && key.sourceVersion().flags().testFlag(QShaderVersion::GlslEs)
                && key.sourceVersion().version() < 320) es32Only.removeShader(key);
        format.setVersion(3, 0);
        QVERIFY(!SnakeMaterial::shadersSupported(es32Only, fragment, QSGRendererInterface::OpenGL, format));
    }

    void rejectedShaderProbeKeepsClassicGeometry()
    {
        static int probes = 0;
        probes = 0;
        SnakeRenderer renderer;
        renderer.m_shaderSupportCheck = [](QQuickWindow *) { ++probes; return false; };
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(24);
        renderer.syncFrame(frame, palette, 0.5, true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        QCOMPARE(node->geometry()->sizeOfVertex(), 12);
        QVERIFY(node->geometry()->vertexCount() > 0);
        QCOMPARE(probes, 1);
        for (int i = 0; i < 10; ++i) {
            renderer.presentFrame(20 + i / 30.0, 0.5);
            node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(node, nullptr));
            QCOMPARE(node->geometry()->sizeOfVertex(), 12);
        }
        QCOMPARE(probes, 1);
        delete node;
    }

    void allocatedVertexCapacityIsInitialized()
    {
        SnakeRenderer view;
        view.setSize(QSizeF(320, 240));
        auto frame = makeFrame(24);
        frame.info.world_width = 320; frame.info.world_height = 240;
        view.syncFrame(frame, palette, 0.5, true);
        auto *node = static_cast<QSGGeometryNode *>(view.updatePaintNode(nullptr, nullptr));
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        const auto *vertices = node->geometry()->vertexDataAsColoredPoint2D();
        QVERIFY(view.m_geometryCapacity > node->geometry()->vertexCount());
        // Rust forms a slice over the full capacity, including this unused tail.
        // MALLOC_PERTURB_ makes this fail on Qt's uninitialized malloc storage.
        for (int i = node->geometry()->vertexCount(); i < view.m_geometryCapacity; ++i) {
            QCOMPARE(vertices[i].x, 0.0f); QCOMPARE(vertices[i].y, 0.0f);
            QCOMPARE(vertices[i].r, uchar(0)); QCOMPARE(vertices[i].a, uchar(0));
        }
        auto *mutableVertices = node->geometry()->vertexDataAsColoredPoint2D();
        const int unused = node->geometry()->vertexCount();
        mutableVertices[unused].x = 123.0f;
        view.presentFrame(frame.info.simulation_time, 0.5);
        node = static_cast<QSGGeometryNode *>(view.updatePaintNode(node, nullptr));
        QCOMPARE(node->geometry()->vertexDataAsColoredPoint2D()[unused].x, 123.0f);
        // Capacity is initialized on growth, never cleared each presentation.
#endif
        delete node;
    }

    void presentationGapsPreserveTransientHistory_data()
    {
        QTest::addColumn<bool>("absolute");
        QTest::addColumn<int>("gap");
        QTest::newRow("relative-three-ticks") << false << 3;
        QTest::newRow("absolute-three-ticks") << true << 3;
        QTest::newRow("relative-ten-ticks") << false << 10;
        QTest::newRow("absolute-ten-ticks") << true << 10;
    }
    void presentationGapsPreserveTransientHistory()
    {
        QFETCH(bool, absolute);
        QFETCH(int, gap);
        SnakeSimulation sim({160, 120, 0, 35, 100, 1000, 0, 7, 6, 0, 1, SNAKES_CORE_RULE_V2, {0}});
        sim.setPresentationLead(4'166'667);
        SnakeRenderer everyTick, skipped;
        for (auto *view : {&everyTick, &skipped}) {
            view->setSize(QSizeF(160, 120)); view->setSimulation(&sim);
        }
        QSGNode *reference = everyTick.updatePaintNode(nullptr, nullptr);
        QSGNode *actual = skipped.updatePaintNode(nullptr, nullptr);
        qint64 time = 1'000'000'000;
        sim.advanceTo(time);
        bool found = false;
        for (int group = 0; group < 300 && !found; ++group) {
            for (int tick = 0; tick < gap; ++tick) {
                time += 33'333'334;
                if (absolute) { sim.advanceTo(time); everyTick.presentAt(time); }
                else sim.advance(1.0 / 30);
                reference = everyTick.updatePaintNode(reference, nullptr);
                if (tick < gap - 1 && std::any_of(sim.frame().events.begin(), sim.frame().events.end(),
                    [](const auto &event) { return event.kind == SNAKES_CORE_EVENT_KILL; })) found = true;
            }
            if (absolute) skipped.presentAt(time);
            actual = skipped.updatePaintNode(actual, nullptr);
        }
        QVERIFY(found); // Death occurred before the final tick of a presentation gap.
        const auto *expected = static_cast<QSGGeometryNode *>(reference)->geometry();
        const auto *observed = static_cast<QSGGeometryNode *>(actual)->geometry();
        QCOMPARE(observed->vertexCount(), expected->vertexCount());
        QCOMPARE(QByteArray(static_cast<const char *>(observed->vertexData()), observed->vertexCount() * 12),
                 QByteArray(static_cast<const char *>(expected->vertexData()), expected->vertexCount() * 12));
        delete actual;
        actual = skipped.updatePaintNode(nullptr, nullptr);
        const auto *recreated = static_cast<QSGGeometryNode *>(actual)->geometry();
        QCOMPARE(recreated->vertexCount(), expected->vertexCount());
        QCOMPARE(QByteArray(static_cast<const char *>(recreated->vertexData()), recreated->vertexCount() * 12),
                 QByteArray(static_cast<const char *>(expected->vertexData()), expected->vertexCount() * 12));
        delete reference; delete actual;
    }

    void batchedAdvancePreservesIntermediateDeath()
    {
        const snakes_core_config config{160, 120, 0, 35, 100, 1000, 0, 7, 6, 0, 1, SNAKES_CORE_RULE_V2, {0}};
        SnakeSimulation referenceSimulation(config), batchedSimulation(config);
        SnakeRenderer everyTick, batched;
        everyTick.setSize(QSizeF(160, 120)); batched.setSize(QSizeF(160, 120));
        everyTick.setSimulation(&referenceSimulation); batched.setSimulation(&batchedSimulation);
        QSGNode *reference = everyTick.updatePaintNode(nullptr, nullptr);
        QSGNode *actual = batched.updatePaintNode(nullptr, nullptr);
        bool found = false;
        for (int group = 0; group < 300 && !found; ++group) {
            for (int tick = 0; tick < 3; ++tick) {
                referenceSimulation.advance(1.0 / 30);
                reference = everyTick.updatePaintNode(reference, nullptr);
                if (tick < 2 && std::any_of(referenceSimulation.frame().events.begin(), referenceSimulation.frame().events.end(),
                    [](const auto &event) { return event.kind == SNAKES_CORE_EVENT_KILL; })) found = true;
            }
            batchedSimulation.advance(0.1);
            actual = batched.updatePaintNode(actual, nullptr);
        }
        QVERIFY(found);
        const auto *expected = static_cast<QSGGeometryNode *>(reference)->geometry();
        const auto *observed = static_cast<QSGGeometryNode *>(actual)->geometry();
        QCOMPARE(observed->vertexCount(), expected->vertexCount());
        QCOMPARE(QByteArray(static_cast<const char *>(observed->vertexData()), observed->vertexCount() * 12),
                 QByteArray(static_cast<const char *>(expected->vertexData()), expected->vertexCount() * 12));
        delete reference; delete actual;
    }

    void perWindowPredictionSurvivesAnotherWindowStepping()
    {
        const snakes_core_config config{320, 240, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, {0}};
        SnakeSimulation sim(config);
        SnakeRenderer first, second;
        first.setSize(QSizeF(320, 240)); second.setSize(QSizeF(320, 240));
        first.setSimulation(&sim); second.setSimulation(&sim);
        constexpr qint64 origin = 1'000'000'000;
        sim.advanceTo(origin);
        sim.advanceTo(origin + 30'000'000);
        first.presentAt(origin + 30'000'000);
        // Cross a physics boundary before the first window's scene-graph sync.
        sim.advanceTo(origin + 40'000'000);
        second.presentAt(origin + 40'000'000);
        auto *firstNode = first.updatePaintNode(nullptr, nullptr);
        auto *secondNode = second.updatePaintNode(nullptr, nullptr);
        QCOMPARE(first.m_frame->info.tick, 0U);
        QCOMPARE(second.m_frame->info.tick, 1U);
        QVERIFY(std::abs(first.m_interpolation - 0.9) < 1e-9);
        QVERIFY(std::abs(second.m_interpolation - 0.2) < 1e-9);
        QVERIFY(std::abs(first.m_simulationTime - 0.03) < 1e-9);
        QVERIFY(std::abs(second.m_simulationTime - 0.04) < 1e-9);
        delete firstNode; delete secondNode;
    }

    void pendingPresentationSurvivesHistoryRecycling()
    {
        SnakeSimulation sim({320, 240, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, {0}});
        SnakeRenderer view;
        view.setSize(QSizeF(320, 240)); view.setSimulation(&sim);
        constexpr qint64 origin = 1'000'000'000;
        sim.advanceTo(origin);
        sim.advanceTo(origin + 30'000'000);
        view.presentAt(origin + 30'000'000);
        // A window can wait arbitrarily long for scene-graph sync. Its pending
        // sample must survive even when every history buffer has been recycled.
        for (int i = 1; i <= 100; ++i) sim.advanceTo(origin + i * 33'333'334LL);
        auto *node = view.updatePaintNode(nullptr, nullptr);
        QCOMPARE(view.m_frame->info.tick, 0U);
        QVERIFY(std::abs(view.m_simulationTime - 0.03) < 1e-9);
        delete node;
    }

    void retainedGeometrySurvivesGrowthShrinkAndMalformedPrimitive()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        SnakeFrame frame;
        QSGNode *node = nullptr;
        const auto render = [&](int count, bool malformed = false) {
            frame = makeFrame(count, 1, 1, malformed);
            frame.info.world_width = 320; frame.info.world_height = 240;
            renderer.syncFrame(frame, palette, 0.5, true);
            node = renderer.updatePaintNode(node, nullptr);
            return static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        };
        const int initial = render(24);
        QVERIFY(initial > 250);
        const int grown = render(420);
        QVERIFY(grown > initial);
        const int capacity = renderer.m_geometryCapacity;
        const int shrunk = render(16);
        QVERIFY(shrunk < grown);
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        QCOMPARE(renderer.m_geometryCapacity, capacity);
#else
        QCOMPARE(capacity, grown);
        QCOMPARE(renderer.m_geometryCapacity, shrunk);
#endif
        const int guarded = render(80, true);
        QVERIFY(guarded > 250);
        QCOMPARE(guarded % 3, 0);
        delete node;
    }
    void resyncReadsNativePreviousPositions()
    {
        SnakeRenderer renderer;
        auto frame = makeFrame(12);
        renderer.syncFrame(frame, palette, 0.5, true);
        auto scaled = makeFrame(12, 1, 1, false, 2);
        renderer.syncFrame(scaled, palette, 0.5, true);
        QCOMPARE(renderer.m_frame, &scaled);
        QCOMPARE(renderer.m_frame->segments[0].x, 500.0f);
        QCOMPARE(renderer.m_frame->segments[0].previous_x, 499.0f);
    }
    void denseFoodDetailUsesHysteresis()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        QSGNode *node = nullptr;
        SnakeFrame frame;
        const auto render = [&](int count) {
            frame = makeFrame(0, 0, count);
            renderer.syncFrame(frame, palette, 0, true);
            node = renderer.updatePaintNode(node, nullptr);
        };
        render(341); QVERIFY(renderer.m_denseFoodRendering);
        render(300); QVERIFY(renderer.m_denseFoodRendering);
        render(279); QVERIFY(!renderer.m_denseFoodRendering);
        delete node;
    }
    void developerSteeringArrowAndViewportScaling()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(320, 240));
        auto frame = makeFrame(18);
        frame.info.world_width = 640; frame.info.world_height = 480;
        renderer.setScaleToViewport(true);
        renderer.syncFrame(frame, palette, 0.5, false);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        const int normal = static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        renderer.setDeveloperMode(true);
        node = renderer.updatePaintNode(node, nullptr);
        QVERIFY(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount() > normal);
        delete node;
    }
    void pausedSharedStateReachesEveryRenderer()
    {
        SnakeSimulation simulation({640, 480, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, {0}});
        simulation.setPaused(true);
        SnakeRenderer first, second;
        first.setSize(QSizeF(320, 240)); second.setSize(QSizeF(320, 240));
        first.setSimulation(&simulation); second.setSimulation(&simulation);
        first.setDeveloperMode(true); second.setDeveloperMode(true);
        QSignalSpy updates(&simulation, &SnakeSimulation::presented);
        QVERIFY(simulation.resize(800, 600));
        QCOMPARE(updates.count(), 1);
        for (auto *renderer : {&first, &second}) {
            QCOMPARE(renderer->m_frame, &simulation.frame());
            QCOMPARE(renderer->m_frame->info.world_width, 800);
            QCOMPARE(renderer->m_frame->info.world_height, 600);
        }
        QTemporaryDir dir;
        Configuration settings(dir.filePath(QStringLiteral("settingsrc")));
        settings.setReducedMotion(true);
        settings.setAnimationPalette(QStringLiteral("ember"));
        simulation.applySettings(settings);
        QCOMPARE(updates.count(), 2);
        QCOMPARE(simulation.frame().info.tick, 0U);
        for (auto *renderer : {&first, &second}) {
            QCOMPARE(renderer->m_palette, SnakeSimulation::colors(QStringLiteral("ember")));
            QCOMPARE(renderer->m_frame, &simulation.frame());
            auto *node = renderer->updatePaintNode(nullptr, nullptr);
            QVERIFY(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount() > 0);
            delete node;
        }
    }

    void sharedNativeFrameAndDestruction()
    {
        auto *simulation = new SnakeSimulation({640, 480, 50, 35, 100, 100, 75, 1, 6, 0, 1, SNAKES_CORE_RULE_DEFAULT, {0}});
        SnakeRenderer a, b;
        a.setSimulation(simulation); b.setSimulation(simulation);
        simulation->advance(1.0 / 30);
        QCOMPARE(a.m_frame, &simulation->frame());
        QCOMPARE(b.m_frame, a.m_frame);
        delete simulation;
        QVERIFY(!a.m_frame); QVERIFY(!b.m_frame);
        QVERIFY(!a.simulation()); QVERIFY(!b.simulation());
    }
    void abiLimitedArenaMapsToEveryViewport_data()
    {
        QTest::addColumn<QString>("behavior");
        QTest::addColumn<QSizeF>("extent");
        QTest::addColumn<bool>("deadly");
        for (const QString &mode : {QStringLiteral("independent"), QStringLiteral("synchronized"), QStringLiteral("seamless")}) {
            for (const QSizeF &extent : {QSizeF(24000, 1200), QSizeF(1600, 24000), QSizeF(24000, 20000)}) {
                for (bool deadly : {true, false}) {
                    const auto name = QStringLiteral("%1-%2x%3-%4").arg(mode).arg(extent.width()).arg(extent.height()).arg(deadly);
                    QTest::newRow(qPrintable(name)) << mode << extent << deadly;
                }
            }
        }
    }
    void abiLimitedArenaMapsToEveryViewport()
    {
        QFETCH(QString, behavior); QFETCH(QSizeF, extent); QFETCH(bool, deadly);
        SnakeSimulation simulation({1280, 720, 50, 35, 100, 100, 75, 1, 6, 0, uint32_t(deadly), SNAKES_CORE_RULE_DEFAULT, {0}});
        QVERIFY(simulation.resize(extent.width(), extent.height()));
        const auto &config = simulation.config();
        SnakeRenderer renderer;
        renderer.setSimulation(&simulation);
        const QSizeF viewport = behavior == QStringLiteral("independent") ? extent : QSizeF(800, 600);
        renderer.setSize(viewport);
        renderer.setScaleToViewport(behavior == QStringLiteral("synchronized"));
        const QPointF offset = behavior == QStringLiteral("seamless")
            ? QPointF(viewport.width() - extent.width(), viewport.height() - extent.height()) : QPointF();
        renderer.setDrawOffset(offset.x(), offset.y());
        // Put both food and a snake near the far desktop edge, beyond 16384
        // logical pixels. They must remain visible on that edge's viewport.
        const QPointF logical(extent.width() - 400, extent.height() - 300);
        const float x = logical.x() * config.width / extent.width();
        const float y = logical.y() * config.height / extent.height();
        SnakeFrame frame;
        frame.info = {0, 0, config.width, config.height, 0, 1, 0, {}, {}};
        frame.food.push_back({1, x, y, 3, 0, 0, x, y, 0, SNAKES_CORE_FOOD_SPARK, 0, 0, 0, 0, 0, 0, 0, 0});
        frame.snakes.push_back({0, 1, 1, 0, 6, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, UINT32_MAX, 0, 0, 0, 0, {}});
        frame.segments.push_back({x, y, x, y});
        frame.segments.push_back({x - 12, y, x - 12, y});
        renderer.syncFrame(frame, palette, 0, deadly);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        auto *geometry = static_cast<QSGGeometryNode *>(node)->geometry();
        QVERIFY(geometry->vertexCount() > 63); // Food plus the culled-copy snake path.
        const auto &center = geometry->vertexDataAsColoredPoint2D()[0];
        const QPointF expected = behavior == QStringLiteral("synchronized")
            ? QPointF(logical.x() * viewport.width() / extent.width(), logical.y() * viewport.height() / extent.height())
            : logical + offset;
        QVERIFY(std::abs(center.x - expected.x()) < 0.01);
        QVERIFY(std::abs(center.y - expected.y()) < 0.01);
        const int normal = geometry->vertexCount();
        renderer.setDeveloperMode(true);
        node = renderer.updatePaintNode(node, nullptr);
        QVERIFY(geometry->vertexCount() > normal);
        delete node;
    }
    void geometryFingerprint_data()
    {
        QTest::addColumn<bool>("deadly");
        QTest::addColumn<bool>("developer");
        QTest::addColumn<double>("alpha");
        QTest::addColumn<QByteArray>("expected");
        QTest::addColumn<int>("fixture");
        QTest::newRow("walls") << true << false << 0.5 << QByteArray("55e68e536a65e2ceb3d5c1258b9a887ef679ae972608acc65bdd7c7c7dc691af") << 0;
        QTest::newRow("wrapping") << false << false << 0.9 << QByteArray("3c7f9798331487979dc67386d05e72b9a0a49737525cac1e1289f449243e7b03") << 0;
        QTest::newRow("developer") << true << true << 0.1 << QByteArray("a7558b4dbf84d518ba4f4de41d104643c2a11a936c7667945212818e05b81ee0") << 0;
        QTest::newRow("sparse-colors") << true << false << 0.0 << QByteArray("a3a8f196ef00d7a96ea0bb9254967d01e95a3d28f3a366b9372f392cef37fef2") << 1;
        // Full screen-space halo bounds add 63 vertices; consistent arena
        // bounds remove an 18-vertex steering copy outside the arena.
        QTest::newRow("offset-scale") << false << true << 1.0 << QByteArray("829b5f8f30a368f48a9d3015907879ebf7f90cffa104370d6f3637da9f973002") << 2;
        QTest::newRow("vacuum-wrap") << false << true << 0.4 << QByteArray("9114668b9724dd9ca5ef5a0d48416da56540d9536a9cae3917e2691815c25b84") << 3;
    }
    void geometryFingerprint()
    {
        QFETCH(bool,deadly);QFETCH(bool,developer);QFETCH(double,alpha);
        QFETCH(QByteArray,expected);QFETCH(int,fixture);
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440,1440));
        renderer.setDeveloperMode(developer);
        auto frame=fixture ? makeFrame(38,3,12) : makeFrame(120,14,400,true);
        auto colors=palette;
        if (fixture==1) {
            colors={QColor(213,61,42,179),QColor(73,123,11,255),QColor(119,17,198,230)};
            for (auto &snake:frame.snakes) {snake.angle=snake.id*0.9;snake.color_index=snake.id;}
            for (auto &food:frame.food) {food.color_index=food.id%3;food.phase=food.id*0.2;}
        } else if (fixture==2) {
            renderer.setSize(QSizeF(640,480));renderer.setScaleToViewport(true);renderer.setDrawOffset(-100,70);
        } else if (fixture==3) {
            frame=makeFrame(38,1,4);
            frame.info.world_width=320;frame.info.world_height=240;
            renderer.setSize(QSizeF(320,240));
            for (auto &seg:frame.segments) {
                seg.x=std::fmod(seg.x+72,320);seg.previous_x=std::fmod(seg.previous_x+72,320);
                seg.y=std::fmod(seg.y+115,240);seg.previous_y=std::fmod(seg.previous_y+115,240);
            }
            for (auto &food:frame.food) {food.x=food.id%2 ? 2:318;food.y=food.id<=2 ? 2:238;food.attraction=0.85;food.attraction_x=6;food.attraction_y=3;}
        }
        renderer.syncFrame(frame,colors,alpha,deadly);
        auto *node=static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr,nullptr));
        auto *g=node->geometry();
        const auto bytes=QByteArrayView(reinterpret_cast<const char *>(g->vertexData()),
            g->vertexCount()*g->sizeOfVertex());
        const auto hash=QCryptographicHash::hash(bytes,QCryptographicHash::Sha256).toHex();
        qInfo() << "geometry" << QTest::currentDataTag() << g->vertexCount() << hash;
        QCOMPARE(hash,expected);
        // Original C++ buffers cover unchanged fixtures. The scaled wrapping
        // fixture intentionally gains full-extent copies after the seam fix.
        const auto directory=qEnvironmentVariable("SNAKES_RENDER_REFERENCE_DIR");
        if (!directory.isEmpty() && fixture!=2) {
            QFile reference(directory + QLatin1Char('/') + QString::fromLatin1(QTest::currentDataTag()) + QStringLiteral(".bin"));
            if (qEnvironmentVariableIsSet("SNAKES_RENDER_WRITE_REFERENCE")) {
                QVERIFY(reference.open(QIODevice::WriteOnly));
                QCOMPARE(reference.write(bytes.data(),bytes.size()),bytes.size());
            } else {
                QVERIFY(reference.open(QIODevice::ReadOnly));
                QCOMPARE(QByteArray(bytes.data(),bytes.size()),reference.readAll());
            }
        }
        delete node;
    }
    void r1FlagsPelletsAndKillEventsReachRustGeometry()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440,1440));
        auto frame=makeFrame(18,1,0);
        frame.snakes[0].flags=0;
        QSGNode *node=nullptr;
        const auto render=[&] {
            renderer.syncFrame(frame,palette,0.5,true);
            node=renderer.updatePaintNode(node,nullptr);
            return static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        };
        const auto hasColor=[&](QColor color) {
            const auto *geometry=static_cast<QSGGeometryNode *>(node)->geometry();
            const auto *vertices=geometry->vertexDataAsColoredPoint2D();
            for (int i=0;i<geometry->vertexCount();++i) {
                const auto &v=vertices[i];
                if (v.r==color.red() && v.g==color.green() && v.b==color.blue() && v.a==color.alpha()) return true;
            }
            return false;
        };
        const int base=render();
        frame.snakes[0].flags=SNAKES_CORE_BOOSTING;++frame.info.tick;
        QCOMPARE(render(),base);
        QVERIFY(hasColor(QColor(139,239,255,245)));
        frame.snakes[0].flags=SNAKES_CORE_HUNTING;++frame.info.tick;
        QCOMPARE(render(),base);
        QVERIFY(hasColor(QColor(255,190,80)));
        frame.snakes[0].flags=SNAKES_CORE_TRAPPED;++frame.info.tick;
        QCOMPARE(render(),base);
        frame.snakes.clear();
        frame.food.push_back({1,200,200,4,0,0,200,200,0,0,255,0, 0, 0, 0, 0, 0, 0});
        QCOMPARE(render(),63);
        frame.food[0].kind=SNAKES_CORE_FOOD_PELLET;
        QCOMPARE(render(),24);
        QVERIFY(hasColor(QColor(77,230,255,110)));
        frame.food.clear();++frame.info.tick;
        frame.events.push_back({frame.info.tick,200,200,0,UINT32_MAX,0,SNAKES_CORE_EVENT_KILL,{0,0,0}, 0, 0, 0, 0, 0, 0});
        QCOMPARE(render(),72);
        renderer.presentFrame(20.2,0.5);node=renderer.updatePaintNode(node,nullptr);
        QCOMPARE(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount(),72);
        renderer.presentFrame(20.5,0.5);node=renderer.updatePaintNode(node,nullptr);
        QCOMPARE(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount(),0);
        delete node;
    }
    void crownFollowsLeaderFlagOnShorterSnake()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440,1440));
        auto frame=makeFrame(30,2,0);
        frame.snakes[0].flags=0;frame.snakes[1].flags=0;
        frame.snakes[1].segment_count=28;
        renderer.syncFrame(frame,palette,0.5,true);
        auto *node=renderer.updatePaintNode(nullptr,nullptr);
        const int base=static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        frame.snakes[1].flags=SNAKES_CORE_LEADER;
        renderer.syncFrame(frame,palette,0.5,true);
        node=renderer.updatePaintNode(node,nullptr);
        QCOMPARE(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount(),base+81);
        delete node;
    }
    void replacingSimulationClearsRustHistoryAtSameTick()
    {
        const snakes_core_config config{320,240,50,35,100,100,75,1,6,0,1,SNAKES_CORE_RULE_DEFAULT,{0}};
        SnakeSimulation first(config),second(config);
        SnakeRenderer renderer,fresh;
        renderer.setSize(QSizeF(320,240));fresh.setSize(QSizeF(320,240));
        renderer.setSimulation(&first);
        SnakeFrame flash;
        flash.info=first.frame().info;
        flash.events.push_back({flash.info.tick,120,120,0,UINT32_MAX,0,SNAKES_CORE_EVENT_KILL,{0,0,0}, 0, 0, 0, 0, 0, 0});
        renderer.syncFrame(flash,palette,0,true);
        auto *node=renderer.updatePaintNode(nullptr,nullptr);
        QCOMPARE(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount(),72);
        renderer.setSimulation(&second);fresh.setSimulation(&second);
        node=renderer.updatePaintNode(node,nullptr);
        auto *reference=fresh.updatePaintNode(nullptr,nullptr);
        const auto *a=static_cast<QSGGeometryNode *>(node)->geometry();
        const auto *b=static_cast<QSGGeometryNode *>(reference)->geometry();
        QCOMPARE(a->vertexCount(),b->vertexCount());
        QCOMPARE(QByteArray(reinterpret_cast<const char *>(a->vertexData()),a->vertexCount()*a->sizeOfVertex()),
                 QByteArray(reinterpret_cast<const char *>(b->vertexData()),b->vertexCount()*b->sizeOfVertex()));
        delete node;delete reference;
    }
    void benchmarkEcosystemPhases_data()
    {
        QTest::addColumn<bool>("deadly");
        QTest::newRow("walls") << true;
        QTest::newRow("wrapping") << false;
    }
    void benchmarkEcosystemPhases()
    {
        QFETCH(bool,deadly);
        snakes_core_config config{3440,1440,100,100,100,100,100,20260814,6,1,uint32_t(deadly),SNAKES_CORE_RULE_DEFAULT,{0}};
        snakes_core_world *handle=nullptr;
        QCOMPARE(snakes_core_create(&config,&handle),SNAKES_CORE_OK);
        std::unique_ptr<snakes_core_world,decltype(&snakes_core_destroy)> world(handle,snakes_core_destroy);
        QVERIFY(world);
        QCOMPARE(snakes_core_step(world.get(),6*1800),SNAKES_CORE_OK);
        SnakeRenderer first,second;
        first.setSize(QSizeF(3440,1440));second.setSize(QSizeF(3440,1440));
        SnakeFrame frame;
        QSGNode *nodes[2]{};
        std::array<std::vector<double>,4> times;
        for (auto &sample:times) sample.reserve(1800);
        uint64_t vertices=0,segments=0,foods=0;
        QElapsedTimer timer;
        for (int tick=0;tick<1800;++tick) {
            timer.start();
            QCOMPARE(snakes_core_step(world.get(),1),SNAKES_CORE_OK);
            times[0].push_back(timer.nsecsElapsed()/1e6);
            timer.restart();
            snakes_core_frame_sizes sizes{};
            QCOMPARE(snakes_core_get_frame_sizes(world.get(),&sizes),SNAKES_CORE_OK);
            frame.snakes.resize(sizes.snakes);frame.segments.resize(sizes.segments);frame.food.resize(sizes.food);
            QCOMPARE(snakes_core_export_frame(world.get(),frame.snakes.data(),frame.snakes.size(),
                frame.segments.data(),frame.segments.size(),frame.food.data(),frame.food.size(),&frame.info),SNAKES_CORE_OK);
            times[1].push_back(timer.nsecsElapsed()/1e6);
            segments+=sizes.segments;foods+=sizes.food;
            int index=0;
            for (auto *view:{&first,&second}) {
                timer.restart();
                view->syncFrame(frame,palette,0.5,deadly);
                nodes[index]=view->updatePaintNode(nodes[index],nullptr);
                times[2+index].push_back(timer.nsecsElapsed()/1e6);
                if (!index) vertices+=static_cast<QSGGeometryNode *>(nodes[index])->geometry()->vertexCount();
                ++index;
            }
        }
        const char *names[]{"step_ms","export_ms","first_window_ms","second_window_ms"};
        for (size_t i=0;i<times.size();++i) {
            auto &sample=times[i];const double mean=std::accumulate(sample.begin(),sample.end(),0.0)/sample.size();
            std::sort(sample.begin(),sample.end());
            qInfo() << names[i] << "mean" << mean << "p50" << sample[900] << "p95" << sample[1710];
        }
        qInfo() << "mean_segments" << segments/1800.0 << "mean_food" << foods/1800.0 << "mean_vertices" << vertices/1800.0;
        delete nodes[0];delete nodes[1];
    }

    void shaderGeometryBudgetAndFrozenTime()
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(24, 1, 1);
        renderer.syncFrame(frame, palette, 0.5, true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        QCOMPARE(node->geometry()->sizeOfVertex(), 24);
        QCOMPARE(node->geometry()->vertexCount(), 24 * 6 + 6);
        auto *material = static_cast<SnakeMaterial *>(node->material());
        QCOMPARE(material->time, float(20 + 0.5 / 30));
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
        const auto *storage = static_cast<const uchar *>(node->geometry()->vertexData());
        for (int i = node->geometry()->vertexCount() * 24; i < renderer.m_geometryCapacity * 24; ++i)
            QCOMPARE(storage[i], uchar(0));
#endif
        renderer.setShaderTimeFrozen(true);
        renderer.presentFrame(21, 0.8);
        renderer.updatePaintNode(node, nullptr);
        QCOMPARE(material->time, float(20 + 0.5 / 30));
        QCOMPARE(material->animationTime, float(20 + 0.5 / 30));
        QCOMPARE(material->motionScale, 0.6f);
        renderer.setShaderTimeFrozen(false);
        renderer.updatePaintNode(node, nullptr);
        QCOMPARE(material->time, 21.0f);
        // A reported scene graph failure recreates the node in classic format.
        renderer.m_shaderFailed = true;
        node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(node, nullptr));
        QCOMPARE(node->geometry()->sizeOfVertex(), 12);
        delete node;
    }

    void reducedMotionFreezesPresentationAnimations_data()
    {
        QTest::addColumn<bool>("shader");
        QTest::addColumn<QString>("scene");
        QTest::addColumn<bool>("geometryAnimated");
        const QStringList scenes{
            QStringLiteral("food-pulse"), QStringLiteral("capsule-1"), QStringLiteral("capsule-2"),
            QStringLiteral("capsule-3"), QStringLiteral("capsule-4"), QStringLiteral("capsule-5"),
            QStringLiteral("surge"), QStringLiteral("magnet"), QStringLiteral("phase"),
            QStringLiteral("contrail"), QStringLiteral("pickup-ring"), QStringLiteral("expiry-ring"),
            QStringLiteral("effect-expiry-ring"), QStringLiteral("kill-flash"), QStringLiteral("succession-ring"),
            QStringLiteral("corpse"), QStringLiteral("crown"), QStringLiteral("head-and-body")};
        for (bool shader : {false, true}) {
            for (const auto &scene : scenes) {
                // Procedural shader animation can change pixels with constant
                // geometry. Classic heads/crowns and succession are static.
                const bool animated = shader ? scene != QStringLiteral("food-pulse")
                        && scene != QStringLiteral("magnet") && scene != QStringLiteral("phase")
                        && scene != QStringLiteral("head-and-body")
                    : scene != QStringLiteral("succession-ring") && scene != QStringLiteral("crown")
                        && scene != QStringLiteral("head-and-body");
                const QByteArray name = (shader ? "shader-" : "classic-") + scene.toUtf8();
                QTest::newRow(name.constData()) << shader << scene << animated;
            }
        }
    }

    void reducedMotionFreezesPresentationAnimations()
    {
        QFETCH(bool, shader);
        QFETCH(QString, scene);
        QFETCH(bool, geometryAnimated);
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = shader;
        auto frame = animationFrame(scene);
        renderer.setSize(QSizeF(frame.info.world_width, frame.info.world_height));
        QSGGeometryNode *node = nullptr;
        if (scene == QStringLiteral("contrail")) {
            // Retain multiple moving tail samples so fading and expiry, rather
            // than just the boost ring, participate in the geometry comparison.
            for (int tick = 596; tick < 600; ++tick) {
                auto history = frame;
                history.info.tick = tick; history.info.simulation_time = tick / 30.0;
                for (auto &point : history.segments) {
                    point.x += (tick - 600) * 2; point.previous_x = point.x;
                }
                renderer.syncFrame(history, palette, 0.5, true);
                node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(node, nullptr));
            }
        }
        renderer.syncFrame(frame, palette, 0.5, true);
        renderer.presentFrame(20.1, 0.5);
        node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(node, nullptr));
        QCOMPARE(node->geometry()->sizeOfVertex(), shader ? 24 : 12);
        QVERIFY(node->geometry()->vertexCount() > 0);
        const auto normal = geometryBytes(node);
        renderer.presentFrame(20.23, 0.5);
        renderer.updatePaintNode(node, nullptr);
        if (geometryAnimated) QVERIFY2(geometryBytes(node) != normal, "Fixture must exercise animated geometry");

        renderer.presentFrame(20.1, 0.5);
        renderer.setShaderTimeFrozen(true);
        renderer.updatePaintNode(node, nullptr);
        const auto frozen = geometryBytes(node);
        for (const double delta : {0.01, 0.07, 0.16, 0.35, 0.65, 1.0, 4.0, 8.0}) {
            renderer.presentFrame(20.1 + delta, 0.5);
            renderer.updatePaintNode(node, nullptr);
            QCOMPARE(geometryBytes(node), frozen);
            if (shader) {
                const auto *material = static_cast<const SnakeMaterial *>(node->material());
                QCOMPARE(material->time, 20.1f);
                QCOMPARE(material->animationTime, 20.1f);
                QCOMPARE(material->motionScale, 0.6f);
                QCOMPARE(material->paletteMode, 0.0f);
            }
        }
        // Disabling reduced motion restores both the flag and normal clocks.
        renderer.setShaderTimeFrozen(false);
        renderer.presentFrame(20.1, 0.5);
        renderer.updatePaintNode(node, nullptr);
        QCOMPARE(geometryBytes(node), normal);
        if (shader) {
            const auto *material = static_cast<const SnakeMaterial *>(node->material());
            QCOMPARE(material->motionScale, 1.0f);
            renderer.presentFrame(20.23, 0.5);
            renderer.updatePaintNode(node, nullptr);
            QCOMPARE(material->time, 20.23f);
            QCOMPARE(material->animationTime, 20.23f);
        }
        delete node;
    }

    void reducedMotionFeastAfterFreeze_data()
    {
        QTest::addColumn<bool>("shader");
        QTest::newRow("classic") << false;
        QTest::newRow("shader") << true;
    }

    void reducedMotionFeastAfterFreeze()
    {
        QFETCH(bool, shader);
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = shader;
        renderer.setSize({3440,1440});
        auto frame = makeFrame(48,1,0);
        frame.snakes[0].flags = 0;
        renderer.syncFrame(frame,palette,0.5,true);
        renderer.setShaderTimeFrozen(true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr,nullptr));
        const auto plain = geometryBytes(node);
        const auto frozenTime = renderer.m_frozenShaderTime;
        frame.info.tick = 705; frame.info.simulation_time = 23.5;
        frame.snakes[0].bulges[0] = {690,55,0,0.35f};
        snakes_core_event feast{};
        feast.tick = 690; feast.snake_id = 0; feast.generation = 1;
        feast.kind = SNAKES_CORE_EVENT_FEAST; feast.duration_ticks = 55;
        frame.events.push_back(feast);
        renderer.syncFrame(frame,palette,0.5,true);
        renderer.presentFrame(23.5,0.5);
        renderer.updatePaintNode(node,nullptr);
        QCOMPARE(geometryBytes(node).size(),plain.size());
        QVERIFY(geometryBytes(node) != plain);
        // Check colour independently of the gulp's wider geometry.
        bool rainbow = false;
        if (shader) {
            const auto *v = static_cast<const snakes_core_shader_vertex *>(node->geometry()->vertexData());
            for (int j=0; j<node->geometry()->vertexCount(); ++j)
                rainbow |= v[j].params[0] == 0 && v[j].params[3] >= 150;
            const auto *material = static_cast<const SnakeMaterial *>(node->material());
            QCOMPARE(material->time,float(frozenTime));
            QCOMPARE(material->animationTime,float(frozenTime));
            QCOMPARE(material->motionScale,0.6f);
        } else {
            const auto *v = node->geometry()->vertexDataAsColoredPoint2D();
            for (int j=0; j<node->geometry()->vertexCount(); ++j)
                rainbow |= v[j].r > palette[0].red() && v[j].g < palette[0].green();
        }
        QVERIFY(rainbow);
        frame.events.clear(); frame.info.tick = 735; frame.info.simulation_time = 24.5;
        renderer.syncFrame(frame,palette,0.5,true);
        renderer.presentFrame(24.5,0.5);
        renderer.updatePaintNode(node,nullptr);
        QCOMPARE(geometryBytes(node),plain);
        delete node;
    }

    void reducedMotionFlagReachesBothFormats_data()
    {
        QTest::addColumn<bool>("shader");
        QTest::newRow("classic") << false;
        QTest::newRow("shader") << true;
    }

    void reducedMotionFlagReachesBothFormats()
    {
        QFETCH(bool, shader);
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = shader;
        const auto frame = powerupsFrame();
        renderer.setSize(QSizeF(1280, 720));
        renderer.syncFrame(frame, palette, 1, true);
        renderer.presentFrame(20.1, 1);
        renderer.setShaderTimeFrozen(true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        const snakes_core_render_params params{1280, 720, 1, 1, 0, 0, 1, 20.1, 1, 0};
        const snakes_core_render_color color{77, 230, 255, 255};
        // Compare with the ABI's reduced-motion styling, so freezing the clock
        // alone cannot mask a missing set_reduced_motion call in either path.
        const auto expected = [&](bool format, bool reduced) {
            std::unique_ptr<snakes_core_renderer, decltype(&snakes_core_render_destroy)> core{
                snakes_core_render_create(), snakes_core_render_destroy};
            snakes_core_render_set_items(core.get(), frame.items.data(), frame.items.size(), frame.itemRadius);
            snakes_core_render_set_reduced_motion(core.get(), uint32_t(reduced));
            snakes_core_render_output out{};
            std::vector<snakes_core_shader_vertex> vertices(65536);
            if (format) snakes_core_render_build_shader(core.get(), &frame.info,
                frame.snakes.data(), frame.snakes.size(), frame.segments.data(), frame.segments.size(),
                frame.food.data(), frame.food.size(), nullptr, 0, &color, 1, &params,
                vertices.data(), vertices.size(), &out);
            else snakes_core_render_build(core.get(), &frame.info,
                frame.snakes.data(), frame.snakes.size(), frame.segments.data(), frame.segments.size(),
                frame.food.data(), frame.food.size(), nullptr, 0, &color, 1, &params,
                reinterpret_cast<snakes_core_render_vertex *>(vertices.data()), vertices.size(), &out);
            return QByteArray(reinterpret_cast<const char *>(vertices.data()), out.vertex_count * (format ? 24 : 12));
        };
        const auto reduced = expected(shader, true);
        QVERIFY(reduced != expected(shader, false));
        QCOMPARE(geometryBytes(node), reduced);
        // Recreated nodes and a shader failure must inherit the same freeze
        // instant and reduced-motion flag even after presentation time advances.
        renderer.presentFrame(24, 1);
        delete node;
        node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        QCOMPARE(geometryBytes(node), reduced);
        if (shader) {
            renderer.m_shaderFailed = true;
            node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(node, nullptr));
            QCOMPARE(node->geometry()->sizeOfVertex(), 12);
            QCOMPARE(geometryBytes(node), expected(false, true));
        }
        delete node;
    }

    void anatomyFixtureUsesPhysicalSpacing()
    {
        const auto frame = anatomyFrame();
        QCOMPARE(frame.snakes.size(), size_t(7));
        QCOMPARE(frame.food.size(), size_t(13));
        for (int k = 0; k < 5; ++k) {
            const auto &snake = frame.snakes[k];
            const auto &head = frame.segments[snake.segment_offset];
            const auto &neck = frame.segments[snake.segment_offset + 1];
            QCOMPARE(snake.angle, std::atan2(head.y - neck.y, head.x - neck.x));
            for (uint32_t j = 1; j < snake.segment_count; ++j) {
                const auto &a = frame.segments[snake.segment_offset + j - 1];
                const auto &b = frame.segments[snake.segment_offset + j];
                QVERIFY(std::abs(std::hypot(a.x-b.x, a.y-b.y) - 1.18*snake.radius) < 0.012);
            }
        }
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(1280, 720));
        renderer.syncFrame(frame, SnakeSimulation::colors(QStringLiteral("ocean")), 1, true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        const auto *geometry = node->geometry();
        QCOMPARE(geometry->sizeOfVertex(), 24);
        std::array<int, 11> counts{};
        const auto *vertices = static_cast<const snakes_core_shader_vertex *>(geometry->vertexData());
        for (int i = 0; i < geometry->vertexCount(); ++i) ++counts[vertices[i].params[0]];
        QCOMPARE(counts[1], 7 * 6);
        QCOMPARE(counts[2], 6 * 6);
        QCOMPARE(counts[3], 3 * 6);
        QCOMPARE(counts[4], 3 * 6);
        QCOMPARE(counts[8], 6);
        QCOMPARE(counts[5], 6);
        qInfo() << "Anatomy fixture vertices" << geometry->vertexCount() << "body vertices" << counts[0]
                << "upload bytes" << geometry->vertexCount() * geometry->sizeOfVertex();
        delete node;
    }

    void itemSpritesUseSixVertices()
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(1280, 720));
        auto frame = makeFrame(2, 1, 0);
        renderer.syncFrame(frame, palette, 1, true);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        const int baseline = static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        for (uint8_t kind = 1; kind <= 3; ++kind) {
            snakes_core_item item{};
            item.id = kind; item.kind = kind;
            item.x = 200 + kind * 180; item.y = 450;
            item.age_ticks = 0; item.life_ticks = 750; frame.itemRadius = 12;
            frame.items.push_back(item);
        }
        renderer.syncFrame(frame, palette, 1, true);
        node = renderer.updatePaintNode(node, nullptr);
        QCOMPARE(static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount(), baseline + 18);
        delete node;
    }

    void powerupFixtureExportsActivePayloads()
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(1280, 720));
        auto frame = powerupsFrame();
        renderer.syncFrame(frame, palette, 1, true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        std::array<int, 16> counts{};
        const auto *geometry = node->geometry();
        const auto *vertices = static_cast<const snakes_core_shader_vertex *>(geometry->vertexData());
        std::array<int, 4> effects{};
        for (int i = 0; i < geometry->vertexCount(); ++i) {
            const auto &v = vertices[i];
            QVERIFY(v.params[0] < counts.size());
            ++counts[v.params[0]];
            if (v.params[0] == 0) ++effects[(v.params[1] >> 2) & 7];
        }
        QCOMPARE(counts[1], 24);
        QCOMPARE(counts[5], 72);
        QCOMPARE(counts[11], 18);
        QCOMPARE(counts[14], 6);
        QCOMPARE(counts[15], 6);
        for (int kind = 1; kind <= 3; ++kind) QCOMPARE(effects[kind], 41 * 6);
        delete node;
    }

private:
    // Two independent snapshots let the capture show all nine moods at both
    // sizes without violating the fourteen-snake ABI or three-bubble cap.
    static SnakeFrame facesFrame(bool adult)
    {
        SnakeFrame frame;
        frame.info = {600, 20, 1280, 720, 0, 1, 0, {}, {}};
        const double r = adult ? 18 : 8.0;
        for (int mood = 0; mood < 9; ++mood) {
            snakes_core_snake snake{};
            snake.id = mood; snake.generation = 1; snake.alive = 1;
            snake.color_index = mood % 6; snake.radius = r;
            snake.segment_offset = frame.segments.size(); snake.segment_count = adult ? 48 : 16;
            snake.mood = mood; snake.mood_intensity = 255; snake.target_item = 255;
            snake.pupil_x = 0.12; snake.pupil_y = -0.12;
            snake.jaw_ticks = mood == SNAKES_CORE_MOOD_SLEEPY ? 20 : 0;
            snake.happy_ticks = mood == SNAKES_CORE_MOOD_HAPPY ? 45 : 0;
            // Compact coils leave room between the grid columns. Head points
            // face right; every point remains finite and within this window.
            const double x = 100 + mood * 132, y = adult ? 370 : 120;
            for (uint32_t j = 0; j < snake.segment_count; ++j) {
                const float px = x - std::min<double>(j * r * 0.7, 45);
                const float py = y + (j > 5 ? 25 + 24 * std::sin((j - 5) * 0.2) : 0);
                frame.segments.push_back({px, py, px, py});
            }
            frame.snakes.push_back(snake);
        }
        frame.info.bubble_count = adult ? 2 : 3;
        for (uint32_t j = 0; j < frame.info.bubble_count; ++j)
            frame.info.bubbles[j] = {j + (adult ? 3u : 0u), 1, 12, uint8_t(j + (adult ? 3 : 0)), 0};
        if (!adult) {
            frame.itemRadius = 18;
            snakes_core_item landing{};
            landing.id = 1; landing.kind = 2; landing.x = 380; landing.y = 630;
            landing.age_ticks = 15; landing.life_ticks = 765; landing.landing_ticks = 15;
            landing.leader_snake_id = UINT32_MAX;
            frame.items.push_back(landing);
            snakes_core_item race{};
            race.id = 2; race.kind = 1; race.x = 870; race.y = 630;
            race.age_ticks = 75; race.life_ticks = 705;
            race.contender_count = 2; race.contender_ids[0] = 6; race.contender_ids[1] = 7;
            race.contender_etas[0] = 1; race.contender_etas[1] = 1.1;
            race.leader_snake_id = 6;
            frame.items.push_back(race);
            // Opposite close contenders at 6r and 8r: c=.912/.853.
            snakes_core_item close = race;
            close.id = 3; close.x = 600; close.y = 630;
            close.contender_ids[0] = 9; close.contender_ids[1] = 10;
            close.leader_snake_id = 9;
            frame.items.push_back(close);
            for (uint32_t id : {9u, 10u}) {
                auto snake = frame.snakes[2];
                snake.id = id; snake.radius = 8; snake.segment_count = 8;
                snake.segment_offset = frame.segments.size();
                snake.angle = id == 9 ? 0 : std::acos(-1.0);
                snake.target_item = 2;
                const float x = close.x + (id == 9 ? -6 : 8) * snake.radius;
                const float direction = id == 9 ? -1 : 1;
                for (uint32_t j = 0; j < snake.segment_count; ++j) {
                    const float px = x + direction * j * snake.radius * 0.7;
                    frame.segments.push_back({px, close.y, px, close.y});
                }
                frame.snakes.push_back(snake);
            }
        }
        return frame;
    }

    // True 1080p proportions: base radius 8.1 px, prize 0.62*base, capsule 2.1*base.
    // REAL_SHIFT offsets gulp/ripe ages; REAL_LIFE tests expiry; REAL_BOOST mixes wave light.
    static SnakeFrame prismFrame()
    {
        constexpr double base = 8.1;
        const qint64 tickShift = qEnvironmentVariableIntValue("REAL_SHIFT");
        const auto shiftedTick = [tickShift](qint64 tick) { return uint64_t(qMax(qint64(0), tick + tickShift)); };
        SnakeFrame frame;
        frame.info = {600, 20, 1280, 720, 0, 1, 0, {}, {}};
        frame.itemRadius = 2.1 * base;
        frame.snakes.reserve(6); // addSnake references remain valid while the fixture is assembled.
        auto addSnake = [&](uint32_t id, uint32_t color, double r, uint32_t len, double hx, double hy,
                            double heading, double amp, double wave) -> snakes_core_snake & {
            snakes_core_snake s{};
            s.id = id; s.generation = 1; s.alive = 1; s.color_index = color; s.radius = r;
            s.angle = heading; s.desired_angle = heading; s.target_item = 255;
            s.grudge_snake_id = UINT32_MAX;
            s.segment_offset = frame.segments.size(); s.segment_count = len;
            const double dx = -std::cos(heading), dy = -std::sin(heading);
            const double nx = -dy, ny = dx;
            for (uint32_t j = 0; j < len; ++j) {
                const double along = j * 1.18 * r;
                const double side = amp * r * std::sin(j * wave);
                const float px = hx + dx * along + nx * side;
                const float py = hy + dy * along + ny * side;
                frame.segments.push_back({px, py, px, py});
            }
            frame.snakes.push_back(s);
            return frame.snakes.back();
        };
        auto addFood = [&](uint64_t id, double x, double y, double size, uint32_t color, uint8_t kind, uint8_t life,
                           uint16_t motion) {
            snakes_core_food f{};
            f.id = id; f.x = x; f.y = y; f.size = size; f.color_index = color; f.kind = kind;
            f.life_fraction = life; f.motion_ticks = motion; f.phase = float(id);
            f.ripe_tick = kind == SNAKES_CORE_FOOD_PRISM_SEED ? 600 + motion : kind == SNAKES_CORE_FOOD_PRISM ? 560 : 0;
            frame.food.push_back(f);
        };
        auto feast = [&](uint32_t id, uint64_t tick, uint16_t duration) {
            snakes_core_event e{};
            e.tick = tick; e.snake_id = id; e.generation = 1;
            e.kind = SNAKES_CORE_EVENT_FEAST; e.duration_ticks = duration;
            frame.events.push_back(e);
        };
        const double prize = 0.62 * base;
        // Contested ripe prism first: prism race arcs use the first seed/fruit.
        addFood(1, 640, 300, prize, 1, SNAKES_CORE_FOOD_PRISM, 250, 0);
        auto &a = addSnake(0, 2, base, 20, 640 - 8 * base, 300 + 2 * base, -0.15, 0.6, 0.35);
        a.face_flags = 4; a.target_item = 255; a.mood = SNAKES_CORE_MOOD_CALM; a.mood_intensity = 255;
        auto &b = addSnake(1, 4, base * 1.05, 26, 640 + 11 * base, 300 - 3 * base, std::acos(-1.0) + 0.2, 0.7, 0.3);
        b.face_flags = 4; b.target_item = 255; b.mood = SNAKES_CORE_MOOD_CALM; b.mood_intensity = 255;
        // Row 1: seeds 0/33/67/100 %, a lone ripe prism, a capsule and ordinary food.
        const double seedX[4] = {110, 240, 370, 500};
        const uint8_t seedLife[4] = {0, 85, 170, 255};
        const uint16_t seedMotion[4] = {90, 60, 30, 1};
        for (int k = 0; k < 4; ++k) addFood(2 + k, seedX[k], 95, prize, 1, SNAKES_CORE_FOOD_PRISM_SEED, seedLife[k], seedMotion[k]);
        addFood(6, 650, 95, prize, 1, SNAKES_CORE_FOOD_PRISM,
                qEnvironmentVariableIsSet("REAL_LIFE") ? uint8_t(qBound(0, qEnvironmentVariableIntValue("REAL_LIFE"), 255)) : 250, 0);
        frame.food.back().ripe_tick = shiftedTick(600);
        snakes_core_item item{};
        item.id = 1; item.kind = 1; item.x = 800; item.y = 95;
        item.age_ticks = 75; item.life_ticks = 705; item.leader_snake_id = UINT32_MAX;
        item.contender_ids[0] = item.contender_ids[1] = UINT32_MAX;
        item.contender_etas[0] = item.contender_etas[1] = INFINITY;
        item.guard_snake_id = UINT32_MAX; item.radius = frame.itemRadius;
        frame.items.push_back(item);
        for (int k = 0; k < 6; ++k) {
            const double value = k < 3 ? 0.5 : 1.4;
            addFood(10 + k, 920 + k * 24, 95, base * (0.23 + value * 0.13), uint32_t(k), SNAKES_CORE_FOOD_SPARK, 255, 0);
        }
        auto &calm = addSnake(2, 0, base, 16, 1150, 95, 0, 0.5, 0.4);
        calm.mood_intensity = 255;
        // Gulps: hatchling early, adult mid, violet late.
        auto &h = addSnake(3, 2, base, 20, 420, 575, 0.05, 0.9, 0.32);
        h.mood = SNAKES_CORE_MOOD_HAPPY; h.mood_intensity = 255; h.happy_ticks = 30;
        h.bulges[0] = {shiftedTick(590), 30, 0, 0.35f};
        feast(3, shiftedTick(590), 30);
        auto &adult = addSnake(4, 0, base * 1.2, 80, 1180, 470, 0.0, 1.4, 0.11);
        adult.mood = SNAKES_CORE_MOOD_HAPPY; adult.mood_intensity = 255; adult.happy_ticks = 30;
        adult.bulges[0] = {shiftedTick(570), 92, 0, 0.35f};
        if (qEnvironmentVariableIsSet("REAL_BOOST")) { adult.flags |= SNAKES_CORE_BOOSTING; h.flags |= SNAKES_CORE_BOOSTING; }
        feast(4, shiftedTick(570), 92);
        auto &v = addSnake(5, 3, base * 1.1, 48, 1180, 640, 0.0, 1.2, 0.16);
        v.mood = SNAKES_CORE_MOOD_HAPPY; v.mood_intensity = 255; v.happy_ticks = 30;
        v.bulges[0] = {shiftedTick(562), 55, 0, 0.35f};
        feast(5, shiftedTick(562), 55);
        frame.info.bubble_count = 1;
        frame.info.bubbles[0] = {4, 1, 15, SNAKES_CORE_GLYPH_HEART, 0};
        return frame;
    }

    static SnakeFrame s1ChaosFrame(bool features)
    {
        // The approved spec's ~8k mature estimate. The older stress fixture
        // (14*120 segments + 400 food) already exceeds the 9.6k chaos ceiling.
        auto frame = makeFrame(90, 14, 300);
        if (!features) return frame;
        frame.snakes[2].bulges[0] = {585,104,0,0.35f};
        frame.snakes[3].bulges[0] = {575,104,0,0.35f};
        snakes_core_event feast{};
        feast.tick=585; feast.snake_id=2; feast.generation=1;
        feast.kind=SNAKES_CORE_EVENT_FEAST; feast.duration_ticks=104;
        frame.events.push_back(feast);
        frame.info.bubble_count = 3;
        for (uint32_t j = 0; j < 3; ++j)
            frame.info.bubbles[j] = {j, 1, 12, uint8_t(j), 0};
        for (auto &snake : frame.snakes) {
            snake.mood = snake.id % 9; snake.mood_intensity = 255;
            snake.pupil_x = 0.15; snake.pupil_y = -0.25;
            if (snake.id < 2) {snake.mood = SNAKES_CORE_MOOD_FROZEN; snake.flags |= SNAKES_CORE_FROZEN;}
        }
        frame.itemRadius = 16;
        for (uint32_t j = 0; j < 3; ++j) {
            snakes_core_item item{};
            item.id = j + 1; item.kind = j + 1; item.x = 800 + 650*j; item.y = 600;
            item.age_ticks = 60; item.life_ticks = 720;
            item.contender_count = 2; item.contender_ids[0] = 2*j; item.contender_ids[1] = 2*j+1;
            item.contender_etas[0] = 1; item.contender_etas[1] = 1.1; item.leader_snake_id = 2*j;
            frame.items.push_back(item);
        }
        return frame;
    }

private Q_SLOTS:
    void facesFixtureAndClockExclusion()
    {
        for (bool adult : {false, true}) {
            SnakeRenderer renderer;
            renderer.m_shaderGeometryForTest = true;
            renderer.setSize({1280, 720});
            const QRectF clock(90, 75, 310, 80);
            renderer.setClockRect(clock);
            auto frame = facesFrame(adult);
            renderer.syncFrame(frame, SnakeSimulation::colors(QStringLiteral("ocean")), 1, true);
            auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
            const auto *g = node->geometry();
            const auto *v = static_cast<const snakes_core_shader_vertex *>(g->vertexData());
            std::array<int, 18> counts{};
            std::array<int, 9> moods{};
            for (int j = 0; j < g->vertexCount(); ++j) {
                ++counts[v[j].params[0]];
                if (v[j].params[0] == 1) ++moods[(v[j].params[2] >> 1) & 15];
                if (v[j].params[0] == 16) {
                    QVERIFY(v[j].x >= 0 && v[j].x <= 1280 && v[j].y >= 0 && v[j].y <= 720);
                    QVERIFY(!clock.contains(QPointF(v[j].x, v[j].y)));
                }
            }
            for (int mood = 0; mood < 9; ++mood) QCOMPARE(moods[mood], !adult && mood == 2 ? 18 : 6);
            QCOMPARE(counts[16], adult ? 12 : 0);
            QCOMPARE(counts[17], adult ? 0 : 24);
            delete node;
        }
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true; renderer.setSize({3440,1440});
        auto frame = s1ChaosFrame(true); renderer.syncFrame(frame, palette, 0.5, true);
        auto *node = static_cast<QSGGeometryNode *>(renderer.updatePaintNode(nullptr, nullptr));
        const auto *g = node->geometry();
        const auto *v = static_cast<const snakes_core_shader_vertex *>(g->vertexData());
        int bubbles = 0, arcs = 0;
        for (int j = 0; j < g->vertexCount(); ++j) {
            bubbles += v[j].params[0] == 16; arcs += v[j].params[0] == 17;
        }
        QCOMPARE(bubbles,18); QCOMPARE(arcs,36); QVERIFY(g->vertexCount() <= 9600);
        qInfo() << "S1 chaos vertices" << g->vertexCount();
        delete node;
    }

    void benchmarkS1MatureGeometry() { benchmarkS1Geometry(false); }
    void benchmarkS1ChaosGeometry() { benchmarkS1Geometry(true); }
private:
    void benchmarkS1Geometry(bool chaos)
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true; renderer.setSize({3440,1440});
        auto frame = s1ChaosFrame(chaos); renderer.syncFrame(frame,palette,0.5,true);
        QSGNode *node = renderer.updatePaintNode(nullptr,nullptr);
        qInfo() << "S1" << (chaos ? "chaos" : "mature") << "vertices"
                << static_cast<QSGGeometryNode *>(node)->geometry()->vertexCount();
        QBENCHMARK {
            renderer.presentFrame(20,0.5);
            node = renderer.updatePaintNode(node,nullptr);
        }
        delete node;
    }

private Q_SLOTS:
    static int bubbleGlyphPixels(const QImage &image, const SnakeFrame &frame, uint32_t index)
    {
        const auto &bubble = frame.info.bubbles[index];
        const auto &snake = frame.snakes[bubble.snake_id];
        const auto &head = frame.segments[snake.segment_offset];
        const double sx = double(image.width()) / frame.info.world_width;
        const double sy = double(image.height()) / frame.info.world_height;
        const int cx = qRound((head.x + 1.9 * snake.radius) * sx);
        const int cy = qRound((head.y - 2.6 * snake.radius) * sy);
        const int rx = qRound(0.5 * snake.radius * sx);
        const int ry = qRound(0.5 * snake.radius * sy);
        int pixels = 0;
        for (int y = cy-ry; y <= cy+ry; ++y)
            for (int x = cx-rx; x <= cx+rx; ++x) {
                const auto pixel = image.pixel(x,y);
                if (qMax(qRed(pixel),qMax(qGreen(pixel),qBlue(pixel))) >= 80) ++pixels;
            }
        return pixels;
    }

    void bubbleCaptureSamplingScalesWithImage_data()
    {
        QTest::addColumn<int>("dpr");
        QTest::newRow("dpr1") << 1;
        QTest::newRow("dpr2") << 2;
    }

    void bubbleCaptureSamplingScalesWithImage()
    {
        QFETCH(int, dpr);
        const auto frame = facesFrame(false);
        QImage image(1280*dpr,720*dpr,QImage::Format_RGB32);
        image.setDevicePixelRatio(dpr);
        image.fill(Qt::black);
        // Paint a synthetic glyph at the known logical centre of bubble zero.
        // The sampling helper must locate it in physical image pixels.
        const auto &snake = frame.snakes[0];
        const int cx = qRound((100 + 1.9 * snake.radius)*dpr);
        const int cy = qRound((120 - 2.6 * snake.radius)*dpr);
        for (int y = cy-1; y <= cy+1; ++y)
            for (int x = cx-1; x <= cx+1; ++x)
                image.setPixel(x,y,qRgb(255,255,255));
        QCOMPARE(bubbleGlyphPixels(image,frame,0),9);
    }

    void captureShaderFixture_data()
    {
        QTest::addColumn<bool>("rejectPipeline");
        QTest::addColumn<bool>("powerups");
        QTest::addColumn<bool>("faces");
        QTest::addColumn<bool>("monoPalette");
        QTest::addColumn<bool>("prism");
        QTest::newRow("shader") << false << false << false << false << false;
        QTest::newRow("driver-rejection-fallback") << true << false << false << false << false;
        QTest::newRow("powerups") << false << true << false << false << false;
        QTest::newRow("faces") << false << false << true << false << false;
        QTest::newRow("faces-mono") << false << false << true << true << false;
        QTest::newRow("prism") << false << false << false << false << true;
        QTest::newRow("prism-mono") << false << false << false << true << true;
    }

    void captureShaderFixture()
    {
        QFETCH(bool, rejectPipeline);
        QFETCH(bool, powerups);
        QFETCH(bool, faces);
        QFETCH(bool, monoPalette);
        QFETCH(bool, prism);
        const auto colors = SnakeSimulation::colors(monoPalette ? QStringLiteral("mono") : QStringLiteral("ocean"));
        const auto path = qEnvironmentVariable("SNAKES_CAPTURE_PATH");
        if (path.isEmpty()) QSKIP("Set SNAKES_CAPTURE_PATH to capture the RHI fixture");
        QQuickWindow window;
        window.resize(1280, 720);
        window.setColor(Qt::black);
        // The borrowed snapshot outlives the item and every render-thread sync.
        SnakeFrame frame;
        SnakeFrame adultFrame;
        SnakeRenderer adultRenderer(window.contentItem());
        SnakeRenderer renderer(window.contentItem());
        if (rejectPipeline) {
            if (window.rendererInterface()->graphicsApi() != QSGRendererInterface::OpenGL)
                QSKIP("The intentional driver compile failure fixture requires OpenGL");
            renderer.m_shaderSupportCheck = [](QQuickWindow *window) {
                const auto *interface = window->rendererInterface();
                auto *rhi = static_cast<QRhi *>(interface->getResource(window, QSGRendererInterface::RhiResource));
                auto *swapchain = static_cast<QRhiSwapChain *>(interface->getResource(window, QSGRendererInterface::RhiSwapchainResource));
                auto vertex = vertexShader();
                // Keep valid reflection and variants but make native GLSL
                // compilation fail inside QRhiGraphicsPipeline::create().
                for (const auto &key : vertex.availableShaders())
                    if (key.source() == QShader::GlslShader)
                        vertex.setShader(key, QShaderCode("this is intentionally invalid GLSL", "main"));
                return SnakeMaterial::probePipelines(rhi, swapchain ? swapchain->currentFrameRenderTarget() : nullptr,
                                                     vertex, fragmentShader());
            };
        }
        renderer.setSize(QSizeF(1280, 720));
        frame = prism ? prismFrame() : faces ? facesFrame(false) : powerups ? powerupsFrame() : anatomyFrame();
        if (faces) {
            adultFrame = facesFrame(true);
            adultRenderer.setSize({1280,720});
            adultRenderer.syncFrame(adultFrame,colors,1,true);
        }
        // Capture the R8 atlas and capsule contours through a real RHI path.
        for (uint8_t kind = 1; !powerups && !faces && !prism && kind <= 3; ++kind) {
            snakes_core_item item{};
            item.id = kind; item.kind = kind;
            item.x = 180 + (kind - 1) * 400; item.y = 650;
            item.age_ticks = 15; item.life_ticks = 750; frame.itemRadius = 18;
            frame.items.push_back(item);
        }
        // Replay 14 earlier physics snapshots to expose the continuous boost
        // contrail. Every visible frame remains deterministic at t=20.
        for (int tick = 586; tick <= 600; ++tick) {
            auto history = frame;
            history.info.tick = tick;
            history.info.simulation_time = tick / 30.0;
            const float shift = (tick - 600) * 2;
            for (const auto &snake : history.snakes) {
                if (!(snake.flags & SNAKES_CORE_BOOSTING)) continue;
                for (uint32_t j = 0; j < snake.segment_count; ++j) {
                    auto &point = history.segments[snake.segment_offset + j];
                    point.x += shift; point.previous_x = point.x;
                }
            }
            renderer.syncFrame(history, colors, 1, true);
            auto &pending = renderer.m_pendingHistory[renderer.m_pendingHistoryHead];
            pending = {};
            pending.info = history.info;
            for (const auto &snake : history.snakes) {
                auto compact = snake;
                compact.segment_offset = pending.snakeCount;
                compact.segment_count = 1;
                pending.snakes[pending.snakeCount] = compact;
                pending.tails[pending.snakeCount++] = history.segments[snake.segment_offset + snake.segment_count - 1];
            }
            renderer.m_pendingHistoryHead = (renderer.m_pendingHistoryHead + 1) % renderer.m_pendingHistory.size();
            ++renderer.m_pendingHistoryCount;
        }
        renderer.syncFrame(frame, colors, 1, true);
        if (prism && qEnvironmentVariableIsSet("REAL_CALM")) renderer.setShaderTimeFrozen(true);
        window.show();
        QVERIFY(QTest::qWaitForWindowExposed(&window));
        QVERIFY(window.rendererInterface()->graphicsApi() != QSGRendererInterface::Software);
        QVERIFY(SnakeMaterial::shadersAvailable());
        const QImage image = window.grabWindow();
        QVERIFY(!image.isNull());
        QCOMPARE(renderer.m_shaderFailed, rejectPipeline);
        QCOMPARE(renderer.m_shaderInUse, !rejectPipeline);
        const QRgb background = image.pixel(0, 0);
        int coloredSamples = 0;
        for (int y = 0; y < image.height(); y += 8)
            for (int x = 0; x < image.width(); x += 8)
                if (image.pixel(x, y) != background) ++coloredSamples;
        QVERIFY2(coloredSamples > 50, "The RHI capture contains no rendered fixture");
        const auto outputPath = rejectPipeline ? path + QStringLiteral(".fallback.png")
            : powerups ? path + QStringLiteral(".powerups.png") : path;
        QVERIFY(image.save(outputPath));
        if (!rejectPipeline) {
            // Inspect only the icon square, excluding the capsule rim, halo
            // and orbiting spark. The dark capsule fill must not pass this
            // check when an atlas is bound but was never created/uploaded.
            const double sx = double(image.width()) / frame.info.world_width;
            const double sy = double(image.height()) / frame.info.world_height;
            for (const auto &item : frame.items) {
                if (item.landing_ticks) continue; // Incoming icon is deliberately faint.
                const int cx = qRound(item.x * sx), cy = qRound(item.y * sy);
                const int rx = qRound(frame.itemRadius * 0.55 * sx);
                const int ry = qRound(frame.itemRadius * 0.55 * sy);
                int iconPixels = 0;
                for (int y = cy - ry; y <= cy + ry; ++y)
                    for (int x = cx - rx; x <= cx + rx; ++x) {
                        const auto pixel = image.pixel(x, y);
                        if (pixel != background && qMax(qRed(pixel), qMax(qGreen(pixel), qBlue(pixel))) >= 96)
                            ++iconPixels;
                    }
                qInfo() << "Capsule kind" << item.kind << "icon pixels" << iconPixels;
                QVERIFY2(iconPixels >= 6, qPrintable(QStringLiteral("Capsule kind %1 has no visible atlas icon (%2 pixels)")
                                                     .arg(item.kind).arg(iconPixels)));
            }
        }
        if (faces) {
            QVERIFY(adultRenderer.m_shaderInUse);
            for (const auto *snapshot : {&frame, &adultFrame}) {
                for (uint32_t j = 0; j < snapshot->info.bubble_count; ++j) {
                    const auto &bubble = snapshot->info.bubbles[j];
                    const int glyphPixels = bubbleGlyphPixels(image,*snapshot,j);
                    QVERIFY2(glyphPixels >= 2,qPrintable(QStringLiteral("Missing bubble glyph %1").arg(bubble.glyph)));
                }
            }
        }
        qInfo() << "Shader fixture:" << outputPath << "RHI API" << window.rendererInterface()->graphicsApi()
                << "classic fallback" << rejectPipeline;
    }

    void benchmarkMatureGeometry()
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(120, 14, 400);
        renderer.syncFrame(frame, palette, 0.5, true);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        const auto *geometry = static_cast<QSGGeometryNode *>(node)->geometry();
        qInfo() << "mature shader vertices" << geometry->vertexCount() << "upload bytes"
                << geometry->vertexCount() * geometry->sizeOfVertex();
        QBENCHMARK {
            renderer.presentFrame(20, 0.5);
            node = renderer.updatePaintNode(node, nullptr);
        }
        delete node;
    }
    void benchmarkPowerupGeometry()
    {
        SnakeRenderer renderer;
        renderer.m_shaderGeometryForTest = true;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(120, 14, 400);
        for (auto &snake : frame.snakes) {
            snake.effect_kind = 1 + snake.id % 3; snake.effect_ticks = 90;
        }
        renderer.syncFrame(frame, palette, 0.5, true);
        QSGNode *node = renderer.updatePaintNode(nullptr, nullptr);
        const auto *geometry = static_cast<QSGGeometryNode *>(node)->geometry();
        qInfo() << "active shader vertices" << geometry->vertexCount() << "upload bytes"
                << geometry->vertexCount() * geometry->sizeOfVertex();
        QBENCHMARK {
            renderer.presentFrame(20, 0.5);
            node = renderer.updatePaintNode(node, nullptr);
        }
        delete node;
    }
    void benchmarkMatureSyncFrame()
    {
        SnakeRenderer renderer;
        renderer.setSize(QSizeF(3440, 1440));
        auto frame = makeFrame(120, 14, 400);
        QBENCHMARK {
            frame.info.simulation_time += 1.0 / 30;
            renderer.syncFrame(frame, palette, 0.5, true);
        }
    }
};
QTEST_MAIN(SnakeRendererTest)
#include "test_snakerenderer.moc"
