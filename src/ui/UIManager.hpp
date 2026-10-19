#pragma once

#include "../painting/LayerStack.hpp"
#include "../painting/Brush.hpp"
#include "../painting/Presets.hpp"
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

    // Viewport Display Options (Substance Painter Mode)
    int displayChannel = 0;       // 0: Material Lit, 1: Base Color, 2: Roughness, 3: Metallic, 4: Normal, 5: Lighting Only
    int environmentPreset = 0;    // 0: Studio, 1: Sunset, 2: Cyberpunk, 3: Forest, 4: Desert
    float environmentRotation = 0.0f; // in radians (controlled by Shift+RMB or UI slider)
    float lightIntensity = 1.0f;

    // Symmetry
    bool symmetryX = false;

    // Export options
    bool showExportModal = false;
    std::string exportDirectory = "/mnt/shared";
    std::string exportPrefix = "REMoAINTER_Mat";

    // Mesh changing requests
    bool requestMeshChange = false;
    int requestedMeshType = 0;
    std::string requestedOBJPath = "";

    // Texture Set settings
    int textureResolution = 1024; // 512, 1024, 2048, 4096

    void render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                bool isMeshHovered, const glm::vec2& hoveredUV,
                std::function<void()> onTriggerComposite);

private:
    void renderMainMenuBar(LayerStack& layerStack);
    void renderTopControlBar(Brush& brush);
    void renderLeftToolbar(Brush& brush);
    void renderLayerStack(LayerStack& layerStack, std::function<void()> onTriggerComposite);
    void renderProperties(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderShelf(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderDisplaySettings(Camera& camera);
    void renderTextureSetList(LayerStack& layerStack);
    void renderViewportOverlay(const Camera& camera, bool isMeshHovered, const glm::vec2& hoveredUV);
    void renderExportDialog(const LayerStack& layerStack);
};

} // namespace REMoAINTER
