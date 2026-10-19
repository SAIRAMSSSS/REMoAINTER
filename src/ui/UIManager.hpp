#pragma once

#include "../painting/LayerStack.hpp"
#include "../painting/Brush.hpp"
#include "../painting/Presets.hpp"
#include "../graphics/Mesh.hpp"
#include "../core/Camera.hpp"
#include "../../third_party/imgui/imgui.h"

#include <functional>
#include <string>
#include <vector>

namespace REMoAINTER {

enum class ToolMode {
    Brush,
    Eraser,
    Projection,
    PolygonFill,
    Smudge,
    Clone,
    Eyedropper
};

class UIManager {
public:
    ToolMode activeTool = ToolMode::Brush;
    std::string statusMessage = "Ready";

    // Viewport Display Options (Substance Painter View Modes)
    int displayChannel = 0;           // 0: Material, 1: Base Color, 2: Roughness, 3: Metallic, 4: Normal, 5: Lighting Only
    int environmentPreset = 1;        // Default to 1: Warm & Cool Studio (matches reference screenshot)
    float environmentRotation = 0.0f; // in radians (controlled by Shift+RMB or UI slider)
    float lightIntensity = 1.0f;

    // Painting Controls
    bool symmetryX = false;
    float strokeOpacity = 1.0f;
    float strokeSpacing = 15.0f; // percentage (15%)

    // Asset Shelf State
    int assetFilterCategory = 0; // 0: All, 1: Materials, 2: Smart Mats, 3: Brushes, 4: Alphas, 5: Envs
    char assetSearchBuffer[64] = "";

    // Layers Panel State
    int layerChannelView = 0; // 0: Base color, 1: Metallic, 2: Roughness, 3: Normal
    int activePropertiesTab = 0; // 0: Brush, 1: Alpha, 2: Stencil, 3: Material

    // Export options
    bool showExportModal = false;
    std::string exportDirectory = "/mnt/shared";
    std::string exportPrefix = "REMoAINTER_Mat";

    // Mesh changing requests
    bool requestMeshChange = false;
    int requestedMeshType = 0;
    std::string requestedOBJPath = "";

    // Texture Set settings
    int textureResolution = 2048; // Default 2048 matches screenshot
    std::string textureSetName = "Head (Main shader)";

    void render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                bool isMeshHovered, const glm::vec2& hoveredUV,
                std::function<void()> onTriggerComposite);

private:
    void renderMainMenuBar(LayerStack& layerStack);
    void renderTopControlBar(Brush& brush);
    void renderLeftToolbar(Brush& brush);
    void renderAssetShelf(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderTextureSetList(LayerStack& layerStack);
    void renderLayerStack(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderPropertiesPanel(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderStatusBar();
    void renderExportDialog(const LayerStack& layerStack);

    // Dynamic Preview Helpers
    void drawStrokePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const Brush& brush);
    void drawMaterialSpherePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const glm::vec4& color, float metallic, float roughness);
    void drawShelfMaterialThumbnail(ImDrawList* drawList, const ImVec2& center, float radius, const glm::vec4& color, float metallic, float roughness);
};

} // namespace REMoAINTER
