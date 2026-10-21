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
    UIManager();

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

    // UI & Layout state
    bool requestLayoutReset = false;
    bool showAboutModal = false;
    ImTextureID logoTextureID = 0;

    // 7 Right Dock Strip Panel Visibility & Focus States
    bool showTextureSetSettings = true;
    bool showTextureSetList = true;
    bool showLayers = true;
    bool showDisplaySettings = true;
    bool showShaderSettings = true;
    bool showHistory = true;
    bool showLog = true;

    bool isPanelVisible[7] = { false, true, true, false, false, false, false };
    int requestFocusPanel = -1;

    // Viewport & Lighting Settings (Display Settings)
    bool displayShadows = true;
    float shadowOpacity = 0.85f;
    float shadowSoftness = 3.0f;
    bool displayWireframe = false;
    int tonemappingMethod = 0; // 0: ACES, 1: Reinhard, 2: Filmic, 3: Linear
    float displayExposure = 1.0f;
    float backgroundBlur = 0.7f;
    bool showEnvironmentBackground = false;

    // Shader Settings
    float roughnessScale = 1.0f;
    float roughnessBias = 0.0f;
    float metallicScale = 1.0f;
    float normalIntensity = 1.0f;
    float aoIntensity = 1.0f;
    bool specularAntiAliasing = true;

    // Texture Set & Baking Settings
    int normalMapFormat = 0; // 0: DirectX, 1: OpenGL
    bool invertNormalY = false;
    float parallaxDepth = 0.02f;
    bool isBaking = false;
    float bakeProgress = 0.0f;

    // History Panel State
    std::vector<std::string> historyActions;
    int currentHistoryIndex = 0;
    void addHistoryAction(const std::string& action);

    // Log Panel State
    struct LogEntry {
        std::string level; // "INFO", "WARN", "ERROR"
        std::string message;
        std::string timestamp;
    };
    std::vector<LogEntry> logEntries;
    bool logFilterInfo = true;
    bool logFilterWarn = true;
    bool logFilterError = true;
    bool logAutoScroll = true;
    char logSearchFilter[64] = "";
    void addLog(const std::string& level, const std::string& message);

    void render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                bool isMeshHovered, const glm::vec2& hoveredUV,
                ImTextureID logoTexture,
                std::function<void()> onTriggerComposite);

private:
    void setupDefaultDockLayout(ImGuiID dockspaceId, const ImVec2& workSize);
    void renderMainMenuBar(LayerStack& layerStack);
    void renderTopControlBar(Brush& brush);
    void renderLeftToolbar(Brush& brush);
    void renderAssetShelf(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderTextureSetSettings(LayerStack& layerStack);
    void renderTextureSetList(LayerStack& layerStack);
    void renderLayerStack(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderDisplaySettings(Camera& camera);
    void renderShaderSettings();
    void renderPropertiesPanel(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite);
    void renderHistoryPanel();
    void renderLogPanel();
    void renderRightDockStrip();
    void renderViewportOverlay(Camera& camera);
    void renderStatusBar();
    void renderExportDialog(const LayerStack& layerStack);
    void renderAboutDialog();

    // Tool Icon & Dynamic Preview Helpers
    void drawToolButton(ToolMode mode, const char* id, const char* label, const char* tooltip, const char* shortcut, bool active, std::function<void()> onClick);
    void drawToolIcon(ImDrawList* drawList, const ImVec2& pMin, const ImVec2& pMax, ToolMode mode, bool active);
    void drawRightStripIcon(ImDrawList* drawList, const ImVec2& pMin, const ImVec2& pMax, int iconIndex, bool active);
    void drawStrokePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const Brush& brush);
    void drawMaterialSpherePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const glm::vec4& color, float metallic, float roughness);
    void drawShelfMaterialThumbnail(ImDrawList* drawList, const ImVec2& center, float radius, const glm::vec4& color, float metallic, float roughness, int badgeType = 0);
};

} // namespace REMoAINTER
