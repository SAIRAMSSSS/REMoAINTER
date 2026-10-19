#pragma once

#include "../painting/LayerStack.hpp"
#include "../painting/Brush.hpp"
#include "../graphics/Mesh.hpp"
#include "../core/Camera.hpp"
#include "../../third_party/imgui/imgui.h"

#include <functional>
#include <string>

namespace REMoAINTER {

enum class ToolMode {
    Brush,
    Eraser,
    Fill,
    Eyedropper
};

class UIManager {
public:
    ToolMode activeTool = ToolMode::Brush;
    std::string statusMessage = "Ready";
    std::string exportDirectory = "./";
    std::string exportPrefix = "Material";

    bool showExportModal = false;
    bool requestMeshChange = false;
    int requestedMeshType = 0; // 0: Sphere, 1: Cube, 2: Torus
    std::string requestedOBJPath = "";

    void render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                bool isMeshHovered, const glm::vec2& hoveredUV,
                std::function<void()> onTriggerComposite);

private:
    void renderMainMenuBar(LayerStack& layerStack);
    void renderToolbar(Brush& brush);
    void renderLayerStack(LayerStack& layerStack, std::function<void()> onTriggerComposite);
    void renderProperties(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderViewportOverlay(const Camera& camera, bool isMeshHovered, const glm::vec2& hoveredUV);
    void renderExportDialog(const LayerStack& layerStack);
};

} // namespace REMoAINTER
