// Native Stage Manager presentation for Hyprland 0.56.2.
// Window buffers stay with the compositor: no readback, screenshots or client resize.
#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/plugins/HookSystem.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/desktop/view/Window.hpp>
#include <hyprland/src/desktop/Workspace.hpp>
#include <hyprland/src/protocols/core/Compositor.hpp>
#include <hyprland/src/managers/SessionLockManager.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopTimer.hpp>
#include <nlohmann/json.hpp>
#include <array>
#include <cmath>
#include <map>
#include <memory>
#include <set>
#include <utility>
#include "perspective.hpp"

using Json = nlohmann::json;
#include "appmenu.hpp"
using Clock = std::chrono::steady_clock;
using Pose = std::array<double, 4>;
static HANDLE handle;
static CHyprSignalListener renderListener;
static SP<CEventLoopTimer> timer;
static SP<SHyprCtlCommand> command, missionCommand;
static CFunctionHook* windowHook = nullptr;

// Keep subscriptions shared when validating/copying a prospective scene.
struct ContentWatch {
    bool dirty = true;
    std::map<CWLSurfaceResource*, std::vector<CHyprSignalListener>> surfaces;
};
struct Item {
    PHLWINDOWREF window;
    std::shared_ptr<ContentWatch> content = std::make_shared<ContentWatch>();
    Pose pose{}, target{}, velocity{};
    double angle = 0., targetAngle = 0., angularVelocity = 0.;
    Clock::time_point last;
    bool settled() const {
        for (size_t i = 0; i < 4; ++i)
            if (std::abs(pose[i] - target[i]) > 0.15 || std::abs(velocity[i]) > 0.5) return false;
        return std::abs(angle - targetAngle) < 0.0005 && std::abs(angularVelocity) < 0.002;
    }
    void step(Clock::time_point now) {
        const double dt = std::chrono::duration<double>(now - last).count();
        last = now;
        // Critically damped spring, analytical and independent of compositor frame rate.
        const double omega = 13., decay = std::exp(-omega * dt);
        for (size_t i = 0; i < 4; ++i) {
            const double offset = pose[i] - target[i], b = velocity[i] + omega * offset;
            pose[i] = target[i] + (offset + b * dt) * decay;
            velocity[i] = (velocity[i] - omega * b * dt) * decay;
        }
        const double offset = angle - targetAngle, b = angularVelocity + omega * offset;
        angle = targetAngle + (offset + b * dt) * decay;
        angularVelocity = (angularVelocity - omega * b * dt) * decay;
        if (settled()) { pose = target; velocity = {}; angle = targetAngle; angularVelocity = 0.; }
    }
};
struct OutputState {
    std::map<std::string, Item> items;
    std::vector<std::string> order;
    std::string output;
    int64_t home = 0;
    int owner = 0;
    uint64_t generation = 0;
    Clock::time_point heartbeat;
    PHLMONITORREF lastMonitor;
};
static std::map<std::string, OutputState> outputs, missionOutputs;
struct RenderStats {
    uint64_t flat = 0, perspective = 0, timerFullDamage = 0, contentDamage = 0;
};
static std::map<std::string, RenderStats> renderStats;

// A single custom pass prevents the normal occlusion pass from culling surfaces
// using their untransformed bounds. All sub-surfaces share one GPU transform.
class StagePass final : public IPassElement {
    PHLWINDOW window;
    PHLMONITOR monitor;
    Pose pose;
    double angle;
public:
    StagePass(PHLWINDOW w, PHLMONITOR m, Pose p, double a) : window(w), monitor(m), pose(p), angle(a) {}
    bool needsLiveBlur() override { return false; }
    bool needsPrecomputeBlur() override { return false; }
    bool disableSimplification() override { return true; }
    const char* passName() override { return "MacstillStage"; }
    ePassElementType type() override { return EK_CUSTOM; }
    std::vector<UP<IPassElement>> surfaces(Pose pose, bool rounded = true) {
        std::vector<UP<IPassElement>> result;
        if (!window->m_isMapped || !window->wlSurface()->resource()) return result;
        const auto size = window->size(Desktop::View::IGeometric::GEOMETRIC_CURRENT);
        if (size.x < 1 || size.y < 1) return result;
        const float scale = std::min(pose[2] / size.x, pose[3] / size.y);
        const auto saved = g_pHyprRenderer->m_renderData.renderModif;
        Render::SRenderModifData mod;
        mod.modifs.emplace_back(Render::SRenderModifData::RMOD_TYPE_SCALE, scale);
        mod.modifs.emplace_back(Render::SRenderModifData::RMOD_TYPE_TRANSLATE,
                                Vector2D{pose[0], pose[1]} * monitor->m_scale);
        result.push_back(makeUnique<CRendererHintsPassElement>(CRendererHintsPassElement::SData{.renderModif = mod}));
        auto root = window->wlSurface()->resource();
        root->breadthfirst([&](SP<CWLSurfaceResource> surface, const Vector2D& offset, void*) {
            if (!surface->m_current.texture || surface->m_current.size.x < 1 || surface->m_current.size.y < 1) return;
            CSurfacePassElement::SRenderData data;
            data.pMonitor = monitor;
            data.when = Time::steadyNow();
            data.pos = monitor->m_position;
            data.localPos = offset;
            data.w = size.x;
            data.h = size.y;
            data.surface = surface;
            data.texture = surface->m_current.texture;
            data.mainSurface = surface == root;
            data.pWindow = window;
            // Surface rendering intersects damage with the *source* box before
            // applying renderModif. Supply the destination scissor explicitly:
            // otherwise large animations lose their right/bottom edges and small
            // previews below the source height are culled entirely.
            data.clipBox = CBox{pose[0], pose[1], size.x * scale, size.y * scale}.scale(monitor->m_scale).round();
            data.dontRound = !rounded;
            data.rounding = window->rounding() * monitor->m_scale;
            data.roundingPower = window->roundingPower();
            result.push_back(makeUnique<CSurfacePassElement>(data));
            surface->frame(data.when);
        }, nullptr);
        result.push_back(makeUnique<CRendererHintsPassElement>(CRendererHintsPassElement::SData{.renderModif = saved}));
        return result;
    }
    std::vector<UP<IPassElement>> draw() override {
        // Flat overview tiles need no full-output intermediate texture. Keep
        // the expensive composition only for Stage's actual perspective poses.
        if (std::abs(angle) < 0.0005 || !perspective.ready()) {
            ++renderStats[monitor->m_name].flat;
            return surfaces(pose);
        }
        auto fb = monitor->resources()->getUnusedWorkBuffer();
        if (!fb) return surfaces(pose);
        ++renderStats[monitor->m_name].perspective;
        const auto size = window->size(Desktop::View::IGeometric::GEOMETRIC_CURRENT);
        if (size.x < 1 || size.y < 1) return {};
        const auto saved = g_pHyprRenderer->m_renderData;
        const auto full = CRegion{CBox{{}, monitor->m_transformedSize}};
        {
            auto guard = g_pHyprRenderer->bindTempFB(fb);
            g_pHyprRenderer->m_renderData.damage = full;
            g_pHyprRenderer->m_renderData.renderModif = {};
            g_pHyprRenderer->m_renderData.clipBox = {};
            g_pHyprRenderer->draw(CClearPassElement::SClearData{CHyprColor(0,0,0,0)}, full);
            // Compose the entire source tree at native output resolution first.
            // The offscreen pass is GPU-only and retains normal surface feedback.
            for (auto& element : surfaces({0., 0., size.x, size.y}, false))
                g_pHyprRenderer->draw(element, full);
        }
        g_pHyprRenderer->m_renderData = saved;
        const double scale = std::min(pose[2] / size.x, pose[3] / size.y);
        // Compensate thumbnail scaling without changing masks at zero yaw.
        double blend = std::clamp((scale - 0.3) / 0.7, 0., 1.);
        blend = blend * blend * (3. - 2. * blend);
        const double radius = (6. * (1. - blend) + window->rounding() * blend) * monitor->m_scale;
        perspective.draw(fb->getTexture(), CBox{0,0,size.x,size.y}.scale(monitor->m_scale),
                         CBox{pose[0],pose[1],size.x*scale,size.y*scale}.scale(monitor->m_scale),
                         std::clamp(angle, 0., 1.4), monitor->m_transformedSize, radius);
        return {};
    }
};

static void clear(OutputState& state) {
    auto& [items, order, output, home, owner, generation, heartbeat, lastMonitor] = state;
    if (auto monitor = lastMonitor.lock()) g_pHyprRenderer->damageMonitor(monitor);
    items.clear(); order.clear(); output.clear(); owner = 0; generation = 0;
}
static bool expired(const OutputState& state) { return Clock::now() - state.heartbeat > std::chrono::seconds(3); }
static bool missionActive(PHLMONITOR monitor) {
    if (!monitor || g_pSessionLockManager->isSessionLocked()) return false;
    auto found = missionOutputs.find(monitor->m_name);
    return found != missionOutputs.end() && !expired(found->second);
}
#include "gestures.hpp"
#include "workspace_background.hpp"
using RenderWindowFn = void(*)(Render::IHyprRenderer*, PHLWINDOW, PHLMONITOR, const Time::steady_tp&, bool, Render::eRenderPassMode, bool, bool);
static void renderWindowHook(Render::IHyprRenderer* renderer, PHLWINDOW window, PHLMONITOR monitor,
                             const Time::steady_tp& time, bool decorate, Render::eRenderPassMode mode, bool ignorePosition, bool standalone) {
    // Mission Control owns exactly one presentation of the live surfaces. Do not
    // submit their untransformed normal passes (including decorations/popups).
    if (!standalone && missionActive(monitor)) return;
    // A transitioning window may already be focused on the home workspace.
    // Keep exactly one presentation until the next scene relinquishes ownership.
    if (!standalone && window && monitor && monitor->m_activeWorkspace && !g_pSessionLockManager->isSessionLocked()) {
        const auto state = outputs.find(monitor->m_name);
        if (state != outputs.end() && !expired(state->second) && monitor->m_activeWorkspace->m_id == state->second.home) {
            const auto item = state->second.items.find(std::format("0x{:x}", reinterpret_cast<uintptr_t>(window.get())));
            if (item != state->second.items.end() && item->second.window.lock() == window) return;
        }
    }
    reinterpret_cast<RenderWindowFn>(windowHook->m_original)(renderer, window, monitor, time, decorate, mode, ignorePosition, standalone);
}
static Pose parsePose(const Json& json) {
    auto p = json.get<Pose>();
    for (double v : p) if (!std::isfinite(v) || std::abs(v) > 100000) throw std::runtime_error("invalid geometry");
    if (p[2] < 1 || p[3] < 1) throw std::runtime_error("empty geometry");
    return p;
}
static std::string request(eHyprCtlOutputFormat, std::string input) {
    try {
        const bool mission = input.starts_with("macstill-mission ");
        auto& outputs = mission ? missionOutputs : ::outputs;
        const auto space = input.find(' ');
        const auto args = space == std::string::npos ? "" : input.substr(space + 1);
        if (args == "menus") return AppMenu::windows().dump();
        if (args == "relationships") {
            Json result = Json::array();
            for (auto w : Desktop::windowState()->windows()) {
                if (!w->m_isMapped) continue;
                auto parent = w->parent();
                result.push_back(Json{{"address", std::format("0x{:x}", reinterpret_cast<uintptr_t>(w.get()))},
                    {"pid", w->getPID()}, {"parent", parent ? Json(std::format("0x{:x}", reinterpret_cast<uintptr_t>(parent.get()))) : Json(nullptr)},
                    {"modal", w->isModal()}});
            }
            return result.dump();
        }
        if (args == "ping") return mission ? "macstill-mission-v1" : "macstill-stage-v4";
        auto data = args == "status" ? Json{{"status", true}} : Json::parse(args);
        if (mission && data.contains("control")) {
            const auto control = data.at("control").get<std::string>();
            const auto file = data.at("gesture_file").get<std::string>();
            const char* runtime = std::getenv("XDG_RUNTIME_DIR");
            if (!runtime || !control.starts_with(std::string(runtime) + "/") ||
                !file.starts_with(std::string(runtime) + "/hypr/")) throw std::runtime_error("invalid gesture transport");
            gestureControl = control; gestureFile = file;
            return "ok";
        }
        // Only the isolated regression compositor opts into synthetic input.
        if (mission && data.contains("test_swipe") && std::getenv("MACSTILL_TEST_GESTURES")) {
            const auto e = data.at("test_swipe");
            const auto phase = e.at("phase").get<std::string>();
            const auto time = e.value("time", uint32_t{1000});
            if (phase == "begin") g_pInputManager->onSwipeBegin({.timeMs=time, .fingers=e.value("fingers", uint32_t{3})});
            else if (phase == "update") g_pInputManager->onSwipeUpdate({.timeMs=time, .fingers=e.value("fingers", uint32_t{3}), .delta={e.value("x", 0.), e.value("y", 0.)}});
            else if (phase == "end") g_pInputManager->onSwipeEnd({.timeMs=time, .cancelled=e.value("cancelled", false)});
            else throw std::runtime_error("invalid swipe phase");
            return "ok";
        }
        const auto name = data.value("output", outputs.empty() ? std::string{} : outputs.begin()->first);
        auto& state = outputs[name];
        auto& [items, order, output, home, owner, generation, heartbeat, lastMonitor] = state;
        if (data.value("status", false)) {
            bool settled = true;
            for (auto& [_, item] : items) { item.step(Clock::now()); settled &= item.settled(); }
            Json poses = Json::array();
            for (const auto& address : order) poses.push_back(Json{{"address", address}, {"rect", items.at(address).pose}, {"angle", items.at(address).angle}});
            const auto& stats = renderStats[name];
            return Json{{"generation", generation}, {"settled", settled}, {"windows", items.size()}, {"owner", owner}, {"poses", poses},
                {"gesture_ready", !gestureControl.empty()}, {"background_slides", backgroundSlides[name]},
                {"render_stats", {{"flat", stats.flat}, {"perspective", stats.perspective},
                                  {"timer_full_damage", stats.timerFullDamage}, {"content_damage", stats.contentDamage}}}}.dump();
        }
        const int pid = data.at("owner").get<int>();
        if (pid <= 0) throw std::runtime_error("invalid owner");
        if (owner && owner != pid && !expired(state)) throw std::runtime_error("another stage session is active");
        if (data.value("clear", false)) { clear(state); outputs.erase(name); return "ok"; }
        auto next = items;
        bool changed = false;
        std::vector<std::string> nextOrder;
        std::set<std::string> seen;
        const auto now = Clock::now();
        const auto entries = data.at("windows");
        if (!entries.is_array() || entries.size() > 256) throw std::runtime_error("too many windows");
        std::map<std::string, PHLWINDOW> live;
        for (auto w : Desktop::windowState()->windows())
            if (w->m_isMapped) live.emplace(std::format("0x{:x}", reinterpret_cast<uintptr_t>(w.get())), w);
        for (const auto& entry : entries) {
            const auto address = entry.at("address").get<std::string>();
            const auto target = parsePose(entry.at("rect"));
            const double angle = entry.value("angle", 0.);
            const double sourceAngle = entry.value("source_angle", angle);
            if (!std::isfinite(angle) || !std::isfinite(sourceAngle) || angle < 0 || angle > 1.4 || sourceAngle < 0 || sourceAngle > 1.4)
                throw std::runtime_error("invalid perspective angle");
            const auto match = live.find(address);
            if (match == live.end()) continue;
            auto window = match->second;
            if (window->getPID() != entry.at("pid").get<int>() || window->m_class != entry.at("class").get<std::string>()) continue;
            auto found = next.find(address);
            if (found == next.end() || found->second.window.lock() != window) {
                changed = true;
                Item item;
                item.window = window; item.pose = entry.contains("source") ? parsePose(entry.at("source")) : target;
                item.target = item.pose; item.last = now; item.angle = item.targetAngle = sourceAngle;
                found = next.insert_or_assign(address, item).first;
            }
            auto& item = found->second;
            changed |= item.target != target || item.targetAngle != angle || !item.settled();
            item.step(now); item.target = target; item.targetAngle = angle;
            if (!data.value("animate", true)) { item.pose = target; item.velocity = {}; item.angle = angle; item.angularVelocity = 0.; }
            seen.insert(address); nextOrder.push_back(address);
        }
        std::erase_if(next, [&](const auto& pair) { return !seen.contains(pair.first); });
        // Validate the complete request before replacing the displayed scene.
        const auto nextOutput = data.at("output").get<std::string>();
        const auto nextHome = data.at("home").get<int64_t>();
        const auto nextGeneration = data.value("generation", uint64_t{0});
        changed |= order != nextOrder || home != nextHome || owner != pid;
        items = std::move(next); order = std::move(nextOrder); output = nextOutput;
        home = nextHome; generation = nextGeneration; owner = pid; heartbeat = now;
        if (changed) if (auto monitor = lastMonitor.lock()) g_pHyprRenderer->damageMonitor(monitor);
        return "ok";
    } catch (const std::exception& error) { return std::string("error: ") + error.what(); }
}

APICALL EXPORT std::string PLUGIN_API_VERSION() { return HYPRLAND_API_VERSION; }
APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE h) {
    handle = h;
    // Never load C++ objects against an incompatible compositor ABI.
    if (std::string(__hyprland_api_get_hash()) != __hyprland_api_get_client_hash())
        throw std::runtime_error("macstill-stage must be built for this Hyprland commit");
    if (!g_pHyprRenderer || g_pHyprRenderer->type() != Render::IHyprRenderer::RT_GL)
        throw std::runtime_error("macstill-stage requires the OpenGL renderer");
    for (const auto& match : HyprlandAPI::findFunctionsByName(handle, "renderWindow")) {
        if (match.demangled.starts_with("Render::IHyprRenderer::renderWindow(")) {
            windowHook = HyprlandAPI::createFunctionHook(handle, match.address, reinterpret_cast<void*>(&renderWindowHook));
            break;
        }
    }
    if (!windowHook || !windowHook->hook()) throw std::runtime_error("Mission Control cannot hook native window presentation");
    for (const auto& match : HyprlandAPI::findFunctionsByName(handle, "renderLayer")) {
        if (match.demangled.starts_with("Render::IHyprRenderer::renderLayer(")) {
            backgroundHook = HyprlandAPI::createFunctionHook(handle, match.address, reinterpret_cast<void*>(&renderDesktopLayer));
            break;
        }
    }
    if (!backgroundHook || !backgroundHook->hook()) throw std::runtime_error("Cannot attach desktop workspace animation");
    AppMenu::init();
    installGestures();
    missionCommand = HyprlandAPI::registerHyprCtlCommand(handle, {"macstill-mission", false, request});
    command = HyprlandAPI::registerHyprCtlCommand(handle, {"macstill-stage", false, request});
    renderListener = Event::bus()->m_events.render.stage.listen([](eRenderStage stage) {
        if ((stage != RENDER_POST_WINDOWS && stage != RENDER_LAST_MOMENT) || g_pSessionLockManager->isSessionLocked()) return;
        auto monitor = g_pHyprRenderer->m_renderData.pMonitor.lock();
        if (!monitor) return;
        const bool mission = missionActive(monitor);
        if (mission ? stage != RENDER_LAST_MOMENT : stage != RENDER_POST_WINDOWS) return;
        auto& outputs = mission ? missionOutputs : ::outputs;
        auto found = outputs.find(monitor->m_name);
        if (found == outputs.end()) return;
        auto& state = found->second;
        auto& [items, order, output, home, owner, generation, heartbeat, lastMonitor] = state;
        lastMonitor = monitor;
        if (items.empty() || expired(state) || !monitor->m_activeWorkspace || (!mission && monitor->m_activeWorkspace->m_id != home)) return;
        for (const auto& address : order) {
            auto& item = items.at(address);
            auto window = item.window.lock();
            // Stage presents parked windows; Mission Control owns all selected surfaces.
            if (!window || !window->m_isMapped || !window->m_workspace ||
                (!mission && window->m_workspace->m_id != home && !window->m_workspace->m_name.starts_with(std::format("macstill-stage-{}-", owner)))) continue;
            // Source damage uses the real window's position, not its thumbnail.
            // Watch every subsurface too (video and embedded surfaces may commit
            // without a root commit), then invalidate the destination on change.
            std::set<CWLSurfaceResource*> present;
            if (auto root = window->wlSurface()->resource()) root->breadthfirst([&](SP<CWLSurfaceResource> surface, const Vector2D&, void*) {
                present.insert(surface.get());
                if (!item.content->surfaces.contains(surface.get())) {
                    std::weak_ptr<ContentWatch> weak = item.content;
                    auto dirty = [weak] {
                        if (auto watch = weak.lock()) watch->dirty = true;
                    };
                    auto& listeners = item.content->surfaces[surface.get()];
                    listeners.push_back(surface->m_events.commit.listen(dirty));
                    listeners.push_back(surface->m_events.unmap.listen(dirty));
                    listeners.push_back(surface->m_events.destroy.listen(dirty));
                    listeners.push_back(surface->m_events.newSubsurface.listen([dirty](SP<CWLSubsurfaceResource>) { dirty(); }));
                }
            }, nullptr);
            std::erase_if(item.content->surfaces, [&](const auto& pair) { return !present.contains(pair.first); });
            item.step(Clock::now());
            g_pHyprRenderer->addPassElement(makeUnique<StagePass>(window, monitor, item.pose, item.angle));
        }
    });
    timer = makeShared<CEventLoopTimer>(std::chrono::milliseconds(16), [](SP<CEventLoopTimer> self, void*) {
        watchTouchpadHolds();
        for (auto* states : {&outputs, &missionOutputs}) std::erase_if(*states, [states](auto& pair) {
            auto& state = pair.second;
            if (expired(state)) { clear(state); return true; }
            if (auto monitor = state.lastMonitor.lock()) {
                // Stage is fully covered while Mission Control owns this output.
                // Retain dirty flags for its next visible frame instead of making
                // hidden perspective previews invalidate the whole overview.
                if (states == &outputs && missionActive(monitor)) return false;
                bool animate = false, perspectiveDirty = false;
                for (auto& [_, item] : state.items) {
                    animate |= !item.settled();
                    if (!std::exchange(item.content->dirty, false)) continue;
                    if (std::abs(item.angle) >= 0.0005) {
                        perspectiveDirty = true;
                    } else {
                        ++renderStats[monitor->m_name].contentDamage;
                        // damageBox takes global logical coordinates.
                        g_pHyprRenderer->damageBox(CBox{monitor->m_position + Vector2D{item.pose[0], item.pose[1]},
                                                       Vector2D{item.pose[2], item.pose[3]}}.expand(2));
                    }
                }
                if (animate || perspectiveDirty) {
                    ++renderStats[monitor->m_name].timerFullDamage;
                    g_pHyprRenderer->damageMonitor(monitor);
                }
            }
            return false;
        });
        self->updateTimeout(std::chrono::milliseconds(16));
    }, nullptr);
    g_pEventLoopManager->addTimer(timer);
    return {"macstill-stage", "Native window surfaces for Stage Manager", "Macstill", "2"};
}
APICALL EXPORT void PLUGIN_EXIT() {
    AppMenu::finish();
    swipeBegin.reset(); swipeUpdate.reset(); swipeEnd.reset();
    holdListeners.clear();
    gestureControl.clear(); gestureFile.clear(); backgroundSlides.clear();
    if (backgroundHook) { backgroundHook->unhook(); backgroundHook = nullptr; }
    timer->cancel(); timer.reset(); renderListener.reset();
    for (auto& [_, state] : outputs) clear(state);
    outputs.clear();
    for (auto& [_, state] : missionOutputs) clear(state);
    missionOutputs.clear();
    renderStats.clear();
    if (windowHook) { windowHook->unhook(); windowHook = nullptr; }
    g_pHyprRenderer->m_renderPass.removeAllOfType("MacstillStage");
    perspective.release();
    HyprlandAPI::unregisterHyprCtlCommand(handle, command); command.reset();
    HyprlandAPI::unregisterHyprCtlCommand(handle, missionCommand); missionCommand.reset();
}
