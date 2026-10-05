// SPDX-License-Identifier: GPL-3.0-or-later
// Headless production frame path. Build via long/appperf CMake driver; C ABI
// wrappers and an instrumented copy of exportFrame supply nested timings.
#include "snakerenderer.h"
#include <QGuiApplication>
#include <QSGGeometryNode>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <thread>

using Clock = std::chrono::steady_clock;
uint64_t perfExportNs = 0, perfStepNs = 0, perfVertexNs = 0, perfSteps = 0;
static uint64_t ns(Clock::duration d) { return std::chrono::duration_cast<std::chrono::nanoseconds>(d).count(); }
static uint64_t cpuNs() { timespec t{}; clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &t); return uint64_t(t.tv_sec)*1000000000+t.tv_nsec; }
extern "C" int32_t __real_snakes_core_step(snakes_core_world *, uint32_t);
extern "C" int32_t __wrap_snakes_core_step(snakes_core_world *w, uint32_t ticks)
{
    const auto t=Clock::now(); const auto result=__real_snakes_core_step(w,ticks);
    perfStepNs+=ns(Clock::now()-t); perfSteps+=ticks; return result;
}
extern "C" int32_t __real_snakes_core_render_build_shader(snakes_core_renderer *, const snakes_core_frame_info *,
    const snakes_core_snake *, size_t, const snakes_core_segment *, size_t, const snakes_core_food *, size_t,
    const snakes_core_event *, size_t, const snakes_core_render_color *, size_t, const snakes_core_render_params *,
    snakes_core_shader_vertex *, size_t, snakes_core_render_output *);
extern "C" int32_t __wrap_snakes_core_render_build_shader(snakes_core_renderer *r, const snakes_core_frame_info *i,
    const snakes_core_snake *s, size_t sn, const snakes_core_segment *b, size_t bn, const snakes_core_food *f, size_t fn,
    const snakes_core_event *e, size_t en, const snakes_core_render_color *c, size_t cn, const snakes_core_render_params *p,
    snakes_core_shader_vertex *v, size_t vn, snakes_core_render_output *o)
{
    const auto t=Clock::now(); const auto result=__real_snakes_core_render_build_shader(r,i,s,sn,b,bn,f,fn,e,en,c,cn,p,v,vn,o);
    perfVertexNs+=ns(Clock::now()-t); return result;
}
class SnakeRendererTest {
public:
    static void shader(SnakeRenderer &r) { r.m_shaderGeometryForTest=true; }
    static QSGNode *build(SnakeRenderer &r, QSGNode *n) { return r.updatePaintNode(n,nullptr); }
    static bool shaderInUse(SnakeRenderer &r) { return r.m_shaderInUse; }
};
struct Sample {
    uint64_t frames=0, gui=0, sync=0, step=0, exports=0, vertices=0, cpu=0, processCpu=0, steps=0, segments=0, geometry=0;
};
int main(int argc, char **argv)
{
    QGuiApplication app(argc,argv);
    bool paced=false, hashGeometry=false; int duration=900;
    for (int i=1;i<argc;++i) {
        if (!std::strcmp(argv[i],"--paced")) paced=true;
        if (!std::strcmp(argv[i],"--hash")) hashGeometry=true;
        if (!std::strcmp(argv[i],"--seconds") && i+1<argc) duration=std::atoi(argv[++i]);
    }
    if (duration<1 || duration>900) {std::fprintf(stderr,"--seconds must be 1..900\n");return 2;}
    snakes_core_config config{3440,1440,80,100,200,300,100,1,6,1,1,SNAKES_CORE_RULE_DEFAULT,{SNAKES_CORE_POWER_UPS_ON}};
    // Allocate the ~large compact history rings on the heap, as production does.
    auto sim=std::make_unique<SnakeSimulation>(config);
    if (!sim->isValid() || !sim->resize(5360,1440) || !sim->resize(7920,1440)) return 2;
    sim->setPresentationLead(20000000);
    const int fps[]={50,58,60}, widths[]={3440,1920,2560}, offsets[]={0,3440,5360};
    std::array<std::unique_ptr<SnakeRenderer>,3> renderers;
    std::array<QSGNode *,3> nodes{};
    std::array<uint64_t,3> frames{}, next{};
    // Non-coincident clocks exercise independent interpolation/history phases.
    const uint64_t phases[]={0,3000000,7000000}; next={phases[0],phases[1],phases[2]};
    for (int i=0;i<3;++i) {
        renderers[i]=std::make_unique<SnakeRenderer>();
        SnakeRendererTest::shader(*renderers[i]); renderers[i]->setSize({qreal(widths[i]),1440});
        renderers[i]->setDrawOffset(-offsets[i],0); renderers[i]->setSimulation(sim.get());
    }
    std::array<Sample,2> samples{}; uint64_t geometryHash=0;
    const auto start=Clock::now(); auto matureStart=start; bool matureStarted=false;
    int previousWindow=-1; uint64_t windowStartCpu=0, progressDeadline=60000000000ULL;
    for (;;) {
        const int i=int(std::min_element(next.begin(),next.end())-next.begin()); const uint64_t t=next[i];
        if (t>=uint64_t(duration)*1000000000) break;
        if (paced && t>=progressDeadline) {
            std::fprintf(stderr,"progress presentation_s=%llu tick=%llu\n",(unsigned long long)(t/1000000000),
                (unsigned long long)sim->frame().info.tick);
            progressDeadline+=60000000000ULL;
        }
        const int window=t<180000000000ULL ? 0 : t>=600000000000ULL ? 1 : -1;
        if (window!=previousWindow) {
            const auto now=cpuNs();
            if (previousWindow>=0) samples[previousWindow].processCpu=now-windowStartCpu;
            if (window>=0) windowStartCpu=now;
            previousWindow=window;
        }
        // Wall pacing covers the measured windows; the middle world still runs
        // every frame/tick, accelerated, then resumes pacing at minute 10.
        if (paced && (t<180000000000ULL || t>=600000000000ULL)) {
            if (t>=600000000000ULL && !matureStarted) {matureStart=Clock::now();matureStarted=true;}
            const auto origin=t<180000000000ULL ? start : matureStart;
            const auto elapsed=t<180000000000ULL ? t : t-600000000000ULL;
            std::this_thread::sleep_until(origin+std::chrono::nanoseconds(elapsed));
        }
        const auto exportBefore=perfExportNs, stepBefore=perfStepNs, vertexBefore=perfVertexNs, stepsBefore=perfSteps;
        const auto cpuBefore=cpuNs(); const auto begin=Clock::now();
        sim->advanceTo(t); renderers[i]->presentAt(t);
        const auto guiEnd=Clock::now(); nodes[i]=SnakeRendererTest::build(*renderers[i],nodes[i]); const auto end=Clock::now();
        const auto cpu=cpuNs()-cpuBefore;
        if (!SnakeRendererTest::shaderInUse(*renderers[i])) { std::fprintf(stderr,"shader path missing\n"); return 3; }
        const auto *geometry=static_cast<QSGGeometryNode *>(nodes[i])->geometry();
        if (window>=0) {
            auto &s=samples[window]; ++s.frames;s.gui+=ns(guiEnd-begin);s.sync+=ns(end-guiEnd);s.cpu+=cpu;
            s.exports+=perfExportNs-exportBefore;s.step+=perfStepNs-stepBefore;s.vertices+=perfVertexNs-vertexBefore;
            s.steps+=perfSteps-stepsBefore; s.segments+=sim->frame().segments.size(); s.geometry+=geometry->vertexCount();
        }
        if (hashGeometry) {
            const auto *data=static_cast<const unsigned char *>(geometry->vertexData());
            for (size_t j=0;j<size_t(geometry->vertexCount())*geometry->sizeOfVertex();++j)
                geometryHash=(geometryHash^data[j])*1099511628211ULL;
        }
        ++frames[i]; next[i]=phases[i]+frames[i]*1000000000ULL/fps[i];
    }
    if (previousWindow>=0) samples[previousWindow].processCpu=cpuNs()-windowStartCpu;
    for (int i=0;i<2;++i) {
        const auto &s=samples[i]; if (!s.frames) continue;
        std::printf("window=%s frames=%llu steps=%llu gui_us=%.6f export_us=%.6f vertex_us=%.6f step_us=%.6f sync_us=%.6f total_us=%.6f cpu_us=%.6f cpu_percent=%.6f process_cpu_percent=%.6f segments=%.3f vertices=%.3f\n",
            i==0?"fresh":"mature",(unsigned long long)s.frames,(unsigned long long)s.steps,
            double(s.gui)/s.frames/1000,double(s.exports)/s.frames/1000,double(s.vertices)/s.frames/1000,
            double(s.step)/s.frames/1000,double(s.sync)/s.frames/1000,double(s.gui+s.sync)/s.frames/1000,
            double(s.cpu)/s.frames/1000,double(s.cpu)/(s.frames/168.0*1e9)*100,
            double(s.processCpu)/(s.frames/168.0*1e9)*100,double(s.segments)/s.frames,double(s.geometry)/s.frames);
    }
    std::printf("paced=%d seconds=%d geometry_hash=%llu tick=%llu\n",paced,duration,(unsigned long long)geometryHash,(unsigned long long)sim->frame().info.tick);
    for (auto *node:nodes) delete node;
}
