#pragma once
#include <hyprland/src/desktop/view/LayerSurface.hpp>
#include <hyprland/src/state/WorkspaceState.hpp>

static CFunctionHook* backgroundHook = nullptr;
static std::map<std::string, uint64_t> backgroundSlides;
using RenderLayerFn = void(*)(Render::IHyprRenderer*, PHLLS, PHLMONITOR, const Time::steady_tp&, bool, bool);

// The desktop texture (wallpaper and icons) participates in exactly the same
// workspace offsets as windows. Menu bar, Dock and overlays remain stationary.
static void renderDesktopLayer(Render::IHyprRenderer* renderer, PHLLS layer, PHLMONITOR monitor,
                               const Time::steady_tp& time, bool popups, bool lockscreen) {
    const auto original = reinterpret_cast<RenderLayerFn>(backgroundHook->m_original);
    if (!layer || layer->m_namespace != "macstill-desktop" || popups || lockscreen ||
        g_pSessionLockManager->isSessionLocked() || missionActive(monitor)) {
        original(renderer, layer, monitor, time, popups, lockscreen);
        return;
    }
    std::vector<PHLWORKSPACE> visible;
    bool moving = false;
    for (auto ws : State::workspaceState()->workspaces()) {
        if (ws->m_monitor != monitor || ws->m_isSpecialWorkspace || ws->m_id <= 0 || ws->m_alpha->value() <= 0.) continue;
        if (!ws->isVisible() && !ws->m_forceRendering && !ws->m_renderOffset->isBeingAnimated()) continue;
        visible.push_back(ws.lock());
        moving |= std::abs(ws->m_renderOffset->value().x) > 0.01;
    }
    if (!moving || visible.empty() || !layer->wlSurface()->resource()) {
        original(renderer, layer, monitor, time, popups, lockscreen);
        return;
    }
    ++backgroundSlides[monitor->m_name];
    const auto root = layer->wlSurface()->resource();
    const auto size = layer->size(Desktop::View::IGeometric::GEOMETRIC_CURRENT);
    for (auto ws : visible) {
        const auto offset = ws->m_renderOffset->value();
        const auto position = layer->position(Desktop::View::IGeometric::GEOMETRIC_CURRENT) + offset;
        root->breadthfirst([&](SP<CWLSurfaceResource> surface, const Vector2D& local, void*) {
            if (!surface->m_current.texture) return;
            CSurfacePassElement::SRenderData data;
            data.pMonitor = monitor;
            data.when = time;
            data.pos = position;
            data.localPos = local;
            data.w = size.x; data.h = size.y;
            data.surface = surface;
            data.texture = surface->m_current.texture;
            data.mainSurface = surface == root;
            data.pLS = layer;
            data.dontRound = true;
            data.fadeAlpha = layer->alpha()[Desktop::View::LS_ALPHA_FADE]->value() * ws->m_alpha->value();
            data.clipBox = CBox{0, 0, monitor->m_size.x, monitor->m_size.y}.scale(monitor->m_scale).round();
            renderer->addPassElement(makeUnique<CSurfacePassElement>(data));
        }, nullptr);
    }
}
