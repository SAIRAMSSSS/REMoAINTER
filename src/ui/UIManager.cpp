#include "UIManager.hpp"
#include "../io/TextureExporter.hpp"
#include <iostream>

namespace REMoAINTER {

void UIManager::render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                       bool isMeshHovered, const glm::vec2& hoveredUV,
                       std::function<void()> onTriggerComposite) {

    // Setup Main DockSpace
    ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImGui::SetNextWindowPos(viewport->WorkPos);
    ImGui::SetNextWindowSize(viewport->WorkSize);
    ImGui::SetNextWindowViewport(viewport->ID);

    ImGuiWindowFlags windowFlags = ImGuiWindowFlags_MenuBar | ImGuiWindowFlags_NoDocking |
                                   ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoCollapse |
                                   ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove |
                                   ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoNavFocus |
                                   ImGuiWindowFlags_NoBackground;

    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 0.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 0.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(0.0f, 0.0f));
    ImGui::Begin("REMoAINTER_DockSpace_Window", nullptr, windowFlags);
    ImGui::PopStyleVar(3);

    ImGuiID dockspaceId = ImGui::GetID("REMoAINTER_Dockspace");
    ImGui::DockSpace(dockspaceId, ImVec2(0.0f, 0.0f), ImGuiDockNodeFlags_PassthruCentralNode);

    renderMainMenuBar(layerStack);
    renderToolbar(brush);
    renderLayerStack(layerStack, onTriggerComposite);
    renderProperties(layerStack, brush, onTriggerComposite);
    renderViewportOverlay(camera, isMeshHovered, hoveredUV);
    renderExportDialog(layerStack);

    ImGui::End();
}

void UIManager::renderMainMenuBar(LayerStack& layerStack) {
    if (ImGui::BeginMenuBar()) {
        if (ImGui::BeginMenu("File")) {
            if (ImGui::MenuItem("New Project", "Ctrl+N")) {
                layerStack.layers.clear();
                layerStack.addFillLayer("Base Material", glm::vec4(0.8f, 0.8f, 0.82f, 1.0f));
                layerStack.addPaintLayer("Paint Layer 1");
                layerStack.composite();
                statusMessage = "New Project Created";
            }
            ImGui::Separator();
            if (ImGui::MenuItem("Export PBR Textures...", "Ctrl+E")) {
                showExportModal = true;
            }
            ImGui::Separator();
            if (ImGui::MenuItem("Exit", "Alt+F4")) {
                exit(0);
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Geometry / Mesh")) {
            if (ImGui::MenuItem("Material Test Sphere")) {
                requestMeshChange = true;
                requestedMeshType = 0;
            }
            if (ImGui::MenuItem("PBR Test Cube")) {
                requestMeshChange = true;
                requestedMeshType = 1;
            }
            if (ImGui::MenuItem("PBR Test Torus")) {
                requestMeshChange = true;
                requestedMeshType = 2;
            }
            ImGui::Separator();
            static char objPathBuffer[256] = "";
            ImGui::TextDisabled("Load Custom Wavefront OBJ:");
            ImGui::InputText("##objpath", objPathBuffer, sizeof(objPathBuffer));
            if (ImGui::Button("Load OBJ File") && strlen(objPathBuffer) > 0) {
                requestMeshChange = true;
                requestedMeshType = 3;
                requestedOBJPath = objPathBuffer;
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Help")) {
            ImGui::MenuItem("REMoAINTER v1.0.0 (Vulkan 1.3)");
            ImGui::TextDisabled("Cross-Platform 3D PBR Texture Painter");
            ImGui::Separator();
            ImGui::BulletText("Alt + Left Mouse: Orbit Camera");
            ImGui::BulletText("Alt + Middle Mouse: Pan Camera");
            ImGui::BulletText("Alt + Right Mouse / Scroll: Zoom Camera");
            ImGui::BulletText("Left Mouse Click/Drag: Paint on 3D Model");
            ImGui::EndMenu();
        }

        ImGui::SameLine(ImGui::GetWindowWidth() - 260);
        ImGui::TextColored(ImVec4(0.85f, 0.55f, 0.20f, 1.0f), "REMoAINTER");
        ImGui::SameLine();
        ImGui::TextDisabled("| Status: %s", statusMessage.c_str());

        ImGui::EndMenuBar();
    }
}

void UIManager::renderToolbar(Brush& brush) {
    ImGui::Begin("Tools##Toolbar", nullptr, ImGuiWindowFlags_NoCollapse);

    if (ImGui::RadioButton("Paint", activeTool == ToolMode::Brush)) {
        activeTool = ToolMode::Brush;
        brush.isEraser = false;
    }
    if (ImGui::RadioButton("Eraser", activeTool == ToolMode::Eraser)) {
        activeTool = ToolMode::Eraser;
        brush.isEraser = true;
    }

    ImGui::Separator();
    ImGui::TextDisabled("Quick Brush Size");
    ImGui::SliderFloat("##quicksize", &brush.size, 1.0f, 128.0f, "%.0f px");

    ImGui::End();
}

void UIManager::renderLayerStack(LayerStack& layerStack, std::function<void()> onTriggerComposite) {
    ImGui::Begin("Layers##LayerStack", nullptr);

    // Layer action buttons
    if (ImGui::Button("+ Paint Layer")) {
        std::string layerName = "Paint Layer " + std::to_string(layerStack.layers.size() + 1);
        layerStack.addPaintLayer(layerName);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("+ Fill Layer")) {
        std::string layerName = "Fill Layer " + std::to_string(layerStack.layers.size() + 1);
        layerStack.addFillLayer(layerName);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("Delete") && layerStack.layers.size() > 1) {
        layerStack.removeLayer(layerStack.activeLayerIndex);
        onTriggerComposite();
    }

    ImGui::Separator();

    // Render layers list (top to bottom order in UI)
    for (int i = (int)layerStack.layers.size() - 1; i >= 0; --i) {
        auto layer = layerStack.layers[i];
        ImGui::PushID(i);

        bool isSelected = (i == layerStack.activeLayerIndex);

        // Visibility checkbox
        if (ImGui::Checkbox("##vis", &layer->visible)) {
            onTriggerComposite();
        }
        ImGui::SameLine();

        // Layer selection button
        std::string label = (layer->type == LayerType::Fill ? "[Fill] " : "[Paint] ") + layer->name;
        if (ImGui::Selectable(label.c_str(), isSelected, ImGuiSelectableFlags_None, ImVec2(160, 0))) {
            layerStack.activeLayerIndex = i;
        }

        if (isSelected) {
            ImGui::Indent(20.0f);

            // Opacity slider
            ImGui::Text("Opacity");
            ImGui::SameLine();
            if (ImGui::SliderFloat("##op", &layer->opacity, 0.0f, 1.0f, "%.2f")) {
                onTriggerComposite();
            }

            // Blend mode selector
            const char* blendModes[] = { "Normal", "Multiply", "Screen", "Overlay", "LinearAdd" };
            int currentMode = (int)layer->blendMode;
            if (ImGui::Combo("Blend", &currentMode, blendModes, IM_ARRAYSIZE(blendModes))) {
                layer->blendMode = (BlendMode)currentMode;
                onTriggerComposite();
            }

            // Channel toggles
            ImGui::Text("Channels:");
            ImGui::SameLine();
            if (ImGui::Checkbox("Color", &layer->channelColor)) onTriggerComposite();
            ImGui::SameLine();
            if (ImGui::Checkbox("Metal", &layer->channelMetallic)) onTriggerComposite();
            ImGui::SameLine();
            if (ImGui::Checkbox("Rough", &layer->channelRoughness)) onTriggerComposite();
            ImGui::SameLine();
            if (ImGui::Checkbox("Normal", &layer->channelNormal)) onTriggerComposite();

            ImGui::Unindent(20.0f);
        }

        ImGui::PopID();
    }

    ImGui::End();
}

void UIManager::renderProperties(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("Properties##PropertiesPanel", nullptr);

    auto activeLayer = layerStack.getActiveLayer();

    if (activeLayer && activeLayer->type == LayerType::Fill) {
        ImGui::TextColored(ImVec4(0.85f, 0.65f, 0.2f, 1.0f), "Fill Layer Material Properties");
        ImGui::Separator();

        if (ImGui::ColorEdit3("Base Color", &activeLayer->fillColor.r)) {
            activeLayer->clear();
            onTriggerComposite();
        }

        if (ImGui::SliderFloat("Metallic", &activeLayer->fillMetallic, 0.0f, 1.0f, "%.3f")) {
            activeLayer->clear();
            onTriggerComposite();
        }

        if (ImGui::SliderFloat("Roughness", &activeLayer->fillRoughness, 0.0f, 1.0f, "%.3f")) {
            activeLayer->clear();
            onTriggerComposite();
        }

    } else {
        ImGui::TextColored(ImVec4(0.35f, 0.75f, 0.95f, 1.0f), "Brush Settings");
        ImGui::Separator();

        ImGui::SliderFloat("Size (Radius)", &brush.size, 1.0f, 128.0f, "%.1f px");
        ImGui::SliderFloat("Hardness", &brush.hardness, 0.0f, 1.0f, "%.2f");
        ImGui::SliderFloat("Opacity", &brush.opacity, 0.0f, 1.0f, "%.2f");
        ImGui::SliderFloat("Flow", &brush.flow, 0.01f, 1.0f, "%.2f");

        ImGui::Spacing();
        ImGui::TextColored(ImVec4(0.35f, 0.75f, 0.95f, 1.0f), "Material Channels to Paint");
        ImGui::Separator();

        ImGui::Checkbox("Paint Base Color", &brush.paintColor);
        if (brush.paintColor) {
            ImGui::ColorEdit3("Color", &brush.color.r);
        }

        ImGui::Checkbox("Paint Metallic", &brush.paintMetallic);
        if (brush.paintMetallic) {
            ImGui::SliderFloat("Metallic Value", &brush.metallic, 0.0f, 1.0f, "%.2f");
        }

        ImGui::Checkbox("Paint Roughness", &brush.paintRoughness);
        if (brush.paintRoughness) {
            ImGui::SliderFloat("Roughness Value", &brush.roughness, 0.0f, 1.0f, "%.2f");
        }

        ImGui::Checkbox("Paint Normal Deviation", &brush.paintNormal);
    }

    ImGui::End();
}

void UIManager::renderViewportOverlay(const Camera& camera, bool isMeshHovered, const glm::vec2& hoveredUV) {
    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs |
                             ImGuiWindowFlags_NoBackground | ImGuiWindowFlags_AlwaysAutoResize;

    ImGui::SetNextWindowPos(ImVec2(75.0f, 35.0f));
    ImGui::Begin("ViewportOverlay", nullptr, flags);

    ImGui::TextColored(ImVec4(0.2f, 0.85f, 0.4f, 1.0f), "Vulkan 1.3 Real-Time PBR Viewport");
    ImGui::TextDisabled("Distance: %.2f | Yaw: %.1f | Pitch: %.1f", camera.distance, camera.yaw, camera.pitch);

    if (isMeshHovered) {
        ImGui::TextColored(ImVec4(0.9f, 0.8f, 0.2f, 1.0f), "Surface UV: (%.3f, %.3f)", hoveredUV.x, hoveredUV.y);
    } else {
        ImGui::TextDisabled("Surface: Background (No Hit)");
    }

    ImGui::End();
}

void UIManager::renderExportDialog(const LayerStack& layerStack) {
    if (!showExportModal) return;

    ImGui::OpenPopup("Export PBR Textures##Modal");
    if (ImGui::BeginPopupModal("Export PBR Textures##Modal", &showExportModal, ImGuiWindowFlags_AlwaysAutoResize)) {
        static char dirBuffer[256] = "/mnt/shared";
        static char prefixBuffer[64] = "REMoAINTER_Mat";

        ImGui::Text("Export your composited PBR texture maps to PNG:");
        ImGui::Separator();

        ImGui::InputText("Output Directory", dirBuffer, sizeof(dirBuffer));
        ImGui::InputText("File Prefix", prefixBuffer, sizeof(prefixBuffer));

        ImGui::BulletText("BaseColor: %s/%s_BaseColor.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Normal:    %s/%s_Normal.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Roughness: %s/%s_Roughness.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Metallic:  %s/%s_Metallic.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("glTF Pack: %s/%s_MetallicRoughness.png", dirBuffer, prefixBuffer);

        ImGui::Separator();
        if (ImGui::Button("Export Textures Now", ImVec2(160, 0))) {
            bool success = TextureExporter::exportPBRMaps(layerStack, dirBuffer, prefixBuffer);
            statusMessage = success ? "Textures Exported Successfully!" : "Export Failed";
            showExportModal = false;
        }
        ImGui::SameLine();
        if (ImGui::Button("Cancel", ImVec2(90, 0))) {
            showExportModal = false;
        }

        ImGui::EndPopup();
    }
}

} // namespace REMoAINTER
