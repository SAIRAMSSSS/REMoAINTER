#include "UIManager.hpp"
#include "../io/TextureExporter.hpp"
#include <iostream>
#include <cmath>

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
    renderTopControlBar(brush);
    renderLeftToolbar(brush);
    renderTextureSetList(layerStack);
    renderLayerStack(layerStack, onTriggerComposite);
    renderProperties(layerStack, brush, onTriggerComposite);
    renderShelf(layerStack, brush, onTriggerComposite);
    renderDisplaySettings(camera);
    renderViewportOverlay(camera, isMeshHovered, hoveredUV);
    renderExportDialog(layerStack);

    ImGui::End();
}

void UIManager::renderMainMenuBar(LayerStack& layerStack) {
    if (ImGui::BeginMenuBar()) {
        if (ImGui::BeginMenu("File")) {
            if (ImGui::MenuItem("New Project...", "Ctrl+N")) {
                layerStack.layers.clear();
                layerStack.addFillLayer("Base Material", glm::vec4(0.8f, 0.8f, 0.82f, 1.0f));
                layerStack.addPaintLayer("Paint Layer 1");
                layerStack.composite();
                statusMessage = "New Project Created";
            }
            ImGui::Separator();
            if (ImGui::MenuItem("Export Textures...", "Ctrl+Shift+E")) {
                showExportModal = true;
            }
            ImGui::Separator();
            if (ImGui::MenuItem("Exit", "Alt+F4")) {
                exit(0);
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Edit")) {
            if (ImGui::MenuItem("Clear Active Layer")) {
                auto active = layerStack.getActiveLayer();
                if (active) {
                    active->clear();
                    layerStack.composite();
                }
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Mode")) {
            if (ImGui::MenuItem("Paint Mode", "1", activeTool == ToolMode::Brush)) activeTool = ToolMode::Brush;
            if (ImGui::MenuItem("Eraser Mode", "2", activeTool == ToolMode::Eraser)) activeTool = ToolMode::Eraser;
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Geometry / Mesh")) {
            if (ImGui::MenuItem("PBR Material Sphere", nullptr, requestedMeshType == 0)) {
                requestMeshChange = true;
                requestedMeshType = 0;
            }
            if (ImGui::MenuItem("PBR Test Cube", nullptr, requestedMeshType == 1)) {
                requestMeshChange = true;
                requestedMeshType = 1;
            }
            if (ImGui::MenuItem("PBR Test Torus", nullptr, requestedMeshType == 2)) {
                requestMeshChange = true;
                requestedMeshType = 2;
            }
            ImGui::Separator();
            static char objPathBuffer[256] = "";
            ImGui::TextDisabled("Load Custom Wavefront OBJ:");
            ImGui::InputText("##objpath", objPathBuffer, sizeof(objPathBuffer));
            if (ImGui::Button("Import 3D Mesh") && strlen(objPathBuffer) > 0) {
                requestMeshChange = true;
                requestedMeshType = 3;
                requestedOBJPath = objPathBuffer;
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Help")) {
            ImGui::MenuItem("REMoAINTER 1.0 (Substance Workflow Edition)");
            ImGui::Separator();
            ImGui::BulletText("Alt + Left Mouse: Orbit 3D Camera");
            ImGui::BulletText("Alt + Middle Mouse: Pan 3D Camera");
            ImGui::BulletText("Alt + Right Mouse / Scroll: Zoom 3D Camera");
            ImGui::BulletText("Shift + Right Mouse Drag: Rotate Environment Lighting");
            ImGui::BulletText("Left Mouse: Paint on 3D Model");
            ImGui::EndMenu();
        }

        // Top-right brand watermark
        ImGui::SameLine(ImGui::GetWindowWidth() - 250);
        ImGui::TextColored(ImVec4(0.92f, 0.45f, 0.15f, 1.0f), "REMoAINTER");
        ImGui::SameLine();
        ImGui::TextDisabled("| %s", statusMessage.c_str());

        ImGui::EndMenuBar();
    }
}

void UIManager::renderTopControlBar(Brush& brush) {
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(8, 4));
    ImGui::Begin("TopBar##ControlBar", nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoScrollbar);

    // Channel Display Selector (Substance Painter Mode switcher)
    ImGui::Text("Channel:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(140);
    const char* channels[] = { "Material", "Base Color", "Roughness", "Metallic", "Normal", "Lighting Only" };
    ImGui::Combo("##viewChannel", &displayChannel, channels, IM_ARRAYSIZE(channels));

    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Environment Lighting Preset
    ImGui::Text("Lighting:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(130);
    const char* envs[] = { "Studio Neutral", "Sunset Warm", "Cyberpunk Neon", "Forest Daylight", "Desert Noon" };
    ImGui::Combo("##envPreset", &environmentPreset, envs, IM_ARRAYSIZE(envs));

    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Quick Brush Sliders
    ImGui::Text("Size:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(90);
    ImGui::SliderFloat("##topsize", &brush.size, 1.0f, 128.0f, "%.0f");

    ImGui::SameLine();
    ImGui::Text("Flow:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(75);
    ImGui::SliderFloat("##topflow", &brush.flow, 0.01f, 1.0f, "%.2f");

    ImGui::SameLine();
    ImGui::Text("Hardness:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(75);
    ImGui::SliderFloat("##tophard", &brush.hardness, 0.0f, 1.0f, "%.2f");

    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Symmetry Mode
    if (ImGui::Button(symmetryX ? "[Symmetry X: ON]" : "[Symmetry X: OFF]")) {
        symmetryX = !symmetryX;
    }

    ImGui::End();
    ImGui::PopStyleVar();
}

void UIManager::renderLeftToolbar(Brush& brush) {
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(4, 4));
    ImGui::Begin("Tools##LeftToolbar", nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize);

    ImVec2 btnSize(38, 38);

    if (ImGui::Button("Paint", btnSize) || (activeTool == ToolMode::Brush && ImGui::IsItemHovered())) {
        activeTool = ToolMode::Brush;
        brush.isEraser = false;
    }
    if (ImGui::IsItemHovered()) ImGui::SetTooltip("Paint Tool (1)");

    if (ImGui::Button("Eraser", btnSize)) {
        activeTool = ToolMode::Eraser;
        brush.isEraser = true;
    }
    if (ImGui::IsItemHovered()) ImGui::SetTooltip("Eraser Tool (2)");

    if (ImGui::Button("Fill", btnSize)) {
        activeTool = ToolMode::Fill;
    }
    if (ImGui::IsItemHovered()) ImGui::SetTooltip("Fill Tool (F)");

    ImGui::End();
    ImGui::PopStyleVar();
}

void UIManager::renderTextureSetList(LayerStack& layerStack) {
    ImGui::Begin("Texture Set List", nullptr);

    ImGui::TextColored(ImVec4(0.85f, 0.55f, 0.20f, 1.0f), "Active Texture Set");
    ImGui::Selectable("Material_Default (Main)", true);

    ImGui::Spacing();
    ImGui::Text("Working Resolution:");
    const char* resolutions[] = { "512 x 512", "1024 x 1024", "2048 x 2048", "4096 x 4096" };
    int currentResIdx = (textureResolution == 512) ? 0 : (textureResolution == 1024 ? 1 : (textureResolution == 2048 ? 2 : 3));
    if (ImGui::Combo("##resCombo", &currentResIdx, resolutions, IM_ARRAYSIZE(resolutions))) {
        int newRes = (currentResIdx == 0) ? 512 : (currentResIdx == 1 ? 1024 : (currentResIdx == 2 ? 2048 : 4096));
        if (newRes != textureResolution) {
            textureResolution = newRes;
            // Resize layer stack
            layerStack.width = newRes;
            layerStack.height = newRes;
            for (auto& l : layerStack.layers) {
                l->resize(newRes, newRes);
            }
            layerStack.composite();
            statusMessage = "Resolution changed to " + std::to_string(newRes) + "x" + std::to_string(newRes);
        }
    }

    ImGui::End();
}

void UIManager::renderLayerStack(LayerStack& layerStack, std::function<void()> onTriggerComposite) {
    ImGui::Begin("Layers", nullptr);

    // Layer action icons / buttons
    if (ImGui::Button("+ Paint")) {
        std::string layerName = "Paint Layer " + std::to_string(layerStack.layers.size() + 1);
        layerStack.addPaintLayer(layerName);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("+ Fill")) {
        std::string layerName = "Fill Layer " + std::to_string(layerStack.layers.size() + 1);
        layerStack.addFillLayer(layerName);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("Delete") && layerStack.layers.size() > 1) {
        layerStack.removeLayer(layerStack.activeLayerIndex);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("Up")) {
        layerStack.moveLayerUp(layerStack.activeLayerIndex);
        onTriggerComposite();
    }
    ImGui::SameLine();
    if (ImGui::Button("Down")) {
        layerStack.moveLayerDown(layerStack.activeLayerIndex);
        onTriggerComposite();
    }

    ImGui::Separator();

    // Render layers from top to bottom
    for (int i = (int)layerStack.layers.size() - 1; i >= 0; --i) {
        auto layer = layerStack.layers[i];
        ImGui::PushID(i);

        bool isSelected = (i == layerStack.activeLayerIndex);

        if (ImGui::Checkbox("##vis", &layer->visible)) {
            onTriggerComposite();
        }
        ImGui::SameLine();

        std::string typeBadge = (layer->type == LayerType::Fill ? "[Fill] " : "[Paint] ");
        if (ImGui::Selectable((typeBadge + layer->name).c_str(), isSelected, ImGuiSelectableFlags_None, ImVec2(170, 0))) {
            layerStack.activeLayerIndex = i;
        }

        if (isSelected) {
            ImGui::Indent(15.0f);

            // Opacity slider
            ImGui::SetNextItemWidth(100);
            if (ImGui::SliderFloat("Opacity", &layer->opacity, 0.0f, 1.0f, "%.2f")) {
                onTriggerComposite();
            }

            // Blend mode
            const char* blendModes[] = { "Normal", "Multiply", "Screen", "Overlay", "LinearAdd" };
            int currentMode = (int)layer->blendMode;
            ImGui::SetNextItemWidth(100);
            if (ImGui::Combo("Blend", &currentMode, blendModes, IM_ARRAYSIZE(blendModes))) {
                layer->blendMode = (BlendMode)currentMode;
                onTriggerComposite();
            }

            // Channel chips: Color, Metal, Rough, Normal
            ImGui::Text("Channels:");
            ImGui::SameLine();
            if (ImGui::SmallButton(layer->channelColor ? "CLR" : "clr")) {
                layer->channelColor = !layer->channelColor;
                onTriggerComposite();
            }
            ImGui::SameLine();
            if (ImGui::SmallButton(layer->channelMetallic ? "MTL" : "mtl")) {
                layer->channelMetallic = !layer->channelMetallic;
                onTriggerComposite();
            }
            ImGui::SameLine();
            if (ImGui::SmallButton(layer->channelRoughness ? "RGH" : "rgh")) {
                layer->channelRoughness = !layer->channelRoughness;
                onTriggerComposite();
            }
            ImGui::SameLine();
            if (ImGui::SmallButton(layer->channelNormal ? "NRM" : "nrm")) {
                layer->channelNormal = !layer->channelNormal;
                onTriggerComposite();
            }

            ImGui::Unindent(15.0f);
        }

        ImGui::PopID();
    }

    ImGui::End();
}

void UIManager::renderProperties(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("Properties", nullptr);

    auto activeLayer = layerStack.getActiveLayer();

    if (activeLayer && activeLayer->type == LayerType::Fill) {
        ImGui::TextColored(ImVec4(0.92f, 0.55f, 0.15f, 1.0f), "Fill Material Properties");
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
        ImGui::TextColored(ImVec4(0.35f, 0.75f, 0.95f, 1.0f), "Brush Parameters");
        ImGui::Separator();

        ImGui::SliderFloat("Size", &brush.size, 1.0f, 128.0f, "%.1f px");
        ImGui::SliderFloat("Hardness", &brush.hardness, 0.0f, 1.0f, "%.2f");
        ImGui::SliderFloat("Opacity", &brush.opacity, 0.0f, 1.0f, "%.2f");
        ImGui::SliderFloat("Flow", &brush.flow, 0.01f, 1.0f, "%.2f");
        ImGui::SliderAngle("Angle", &brush.angle, -180.0f, 180.0f);

        const char* alphaNames[] = { "Circle Hard", "Circle Soft", "Grunge Noise", "Scratches", "Stipple Grid", "Square Box", "Star Burst" };
        ImGui::Combo("Alpha Mask", &brush.alphaType, alphaNames, IM_ARRAYSIZE(alphaNames));

        ImGui::Spacing();
        ImGui::TextColored(ImVec4(0.35f, 0.75f, 0.95f, 1.0f), "Material Channels to Paint");
        ImGui::Separator();

        ImGui::Checkbox("Color", &brush.paintColor);
        if (brush.paintColor) {
            ImGui::SameLine();
            ImGui::ColorEdit3("##colorVal", &brush.color.r, ImGuiColorEditFlags_NoInputs);
        }

        ImGui::Checkbox("Metallic", &brush.paintMetallic);
        if (brush.paintMetallic) {
            ImGui::SliderFloat("##metalVal", &brush.metallic, 0.0f, 1.0f, "%.3f");
        }

        ImGui::Checkbox("Roughness", &brush.paintRoughness);
        if (brush.paintRoughness) {
            ImGui::SliderFloat("##roughVal", &brush.roughness, 0.0f, 1.0f, "%.3f");
        }

        ImGui::Checkbox("Normal Deviation", &brush.paintNormal);
    }

    ImGui::End();
}

void UIManager::renderShelf(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("Shelf (Assets)", nullptr);

    if (ImGui::BeginTabBar("ShelfTabs")) {
        // Tab 1: Materials
        if (ImGui::BeginTabItem("Materials")) {
            auto materials = PresetLibrary::getMaterials();

            ImGui::BeginChild("MaterialGrid", ImVec2(0, 0), true);
            float panelWidth = ImGui::GetContentRegionAvail().x;
            int columns = std::max(1, (int)(panelWidth / 120.0f));
            ImGui::Columns(columns, nullptr, false);

            for (size_t i = 0; i < materials.size(); ++i) {
                const auto& mat = materials[i];
                ImGui::PushID((int)i);

                // Material color chip button
                ImVec4 btnColor(mat.baseColor.r, mat.baseColor.g, mat.baseColor.b, 1.0f);
                ImGui::PushStyleColor(ImGuiCol_Button, btnColor);
                ImGui::PushStyleColor(ImGuiCol_ButtonHovered, ImVec4(btnColor.x * 1.2f, btnColor.y * 1.2f, btnColor.z * 1.2f, 1.0f));
                ImGui::PushStyleColor(ImGuiCol_ButtonActive, ImVec4(btnColor.x * 0.8f, btnColor.y * 0.8f, btnColor.z * 0.8f, 1.0f));

                if (ImGui::Button("##matbtn", ImVec2(80, 50))) {
                    auto activeLayer = layerStack.getActiveLayer();
                    if (activeLayer && activeLayer->type == LayerType::Fill) {
                        activeLayer->fillColor = mat.baseColor;
                        activeLayer->fillMetallic = mat.metallic;
                        activeLayer->fillRoughness = mat.roughness;
                        activeLayer->clear();
                        onTriggerComposite();
                    } else {
                        brush.color = mat.baseColor;
                        brush.metallic = mat.metallic;
                        brush.roughness = mat.roughness;
                    }
                    statusMessage = "Loaded Material: " + mat.name;
                }
                ImGui::PopStyleColor(3);

                ImGui::TextWrapped("%s", mat.name.c_str());
                ImGui::TextDisabled("M:%.2f R:%.2f", mat.metallic, mat.roughness);

                ImGui::NextColumn();
                ImGui::PopID();
            }

            ImGui::Columns(1);
            ImGui::EndChild();
            ImGui::EndTabItem();
        }

        // Tab 2: Brushes
        if (ImGui::BeginTabItem("Brushes")) {
            auto brushes = PresetLibrary::getBrushes();

            for (size_t i = 0; i < brushes.size(); ++i) {
                const auto& b = brushes[i];
                ImGui::PushID((int)i);
                if (ImGui::Button(b.name.c_str(), ImVec2(120, 32))) {
                    brush.size = b.size;
                    brush.hardness = b.hardness;
                    brush.opacity = b.opacity;
                    brush.flow = b.flow;
                    brush.alphaType = b.alphaIndex;
                    statusMessage = "Active Brush: " + b.name;
                }
                if ((i + 1) % 4 != 0 && (i + 1) < brushes.size()) ImGui::SameLine();
                ImGui::PopID();
            }
            ImGui::EndTabItem();
        }

        // Tab 3: Alphas
        if (ImGui::BeginTabItem("Alphas")) {
            auto alphas = PresetLibrary::getAlphas();

            for (size_t i = 0; i < alphas.size(); ++i) {
                const auto& a = alphas[i];
                ImGui::PushID((int)i);
                bool isSelected = (brush.alphaType == a.type);
                if (ImGui::Button(a.name.c_str(), ImVec2(120, 32))) {
                    brush.alphaType = a.type;
                    statusMessage = "Active Alpha: " + a.name;
                }
                if ((i + 1) % 4 != 0 && (i + 1) < alphas.size()) ImGui::SameLine();
                ImGui::PopID();
            }
            ImGui::EndTabItem();
        }

        ImGui::EndTabBar();
    }

    ImGui::End();
}

void UIManager::renderDisplaySettings(Camera& camera) {
    ImGui::Begin("Display Settings", nullptr);

    ImGui::TextColored(ImVec4(0.85f, 0.55f, 0.20f, 1.0f), "Environment & Lighting");
    ImGui::Separator();

    const char* envs[] = { "Studio Neutral", "Sunset Warm", "Cyberpunk Neon", "Forest Daylight", "Desert Noon" };
    ImGui::Combo("Environment Map", &environmentPreset, envs, IM_ARRAYSIZE(envs));

    // Rotation slider (and Shift+RMB reminder)
    float rotDeg = glm::degrees(environmentRotation);
    if (ImGui::SliderFloat("Rotation", &rotDeg, 0.0f, 360.0f, "%.1f deg")) {
        environmentRotation = glm::radians(rotDeg);
    }
    ImGui::TextDisabled("(Shortcut: Hold Shift + Right Mouse Drag)");

    ImGui::SliderFloat("Light Intensity", &lightIntensity, 0.1f, 4.0f, "%.2f");

    ImGui::Spacing();
    ImGui::TextColored(ImVec4(0.85f, 0.55f, 0.20f, 1.0f), "Camera");
    ImGui::Separator();
    ImGui::SliderFloat("Field of View", &camera.fov, 20.0f, 90.0f, "%.0f deg");
    if (ImGui::Button("Reset Camera View")) {
        camera.distance = 3.5f;
        camera.yaw = 45.0f;
        camera.pitch = 25.0f;
        camera.target = glm::vec3(0.0f);
    }

    ImGui::End();
}

void UIManager::renderViewportOverlay(const Camera& camera, bool isMeshHovered, const glm::vec2& hoveredUV) {
    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs |
                             ImGuiWindowFlags_NoBackground | ImGuiWindowFlags_AlwaysAutoResize;

    ImGui::SetNextWindowPos(ImVec2(60.0f, 65.0f));
    ImGui::Begin("ViewportOverlay", nullptr, flags);

    const char* channelNames[] = { "Material (PBR Lit)", "Solo: Base Color", "Solo: Roughness", "Solo: Metallic", "Solo: Normal", "Lighting Only" };
    ImGui::TextColored(ImVec4(0.92f, 0.55f, 0.15f, 1.0f), "Viewport: %s", channelNames[displayChannel]);
    ImGui::TextDisabled("Env Rotation: %.1f deg | Light: %.2fx", glm::degrees(environmentRotation), lightIntensity);

    if (isMeshHovered) {
        ImGui::TextColored(ImVec4(0.2f, 0.85f, 0.4f, 1.0f), "UV: (%.3f, %.3f)", hoveredUV.x, hoveredUV.y);
    } else {
        ImGui::TextDisabled("UV: None");
    }

    ImGui::End();
}

void UIManager::renderExportDialog(const LayerStack& layerStack) {
    if (!showExportModal) return;

    ImGui::OpenPopup("Export PBR Textures##Modal");
    if (ImGui::BeginPopupModal("Export PBR Textures##Modal", &showExportModal, ImGuiWindowFlags_AlwaysAutoResize)) {
        static char dirBuffer[256] = "/mnt/shared";
        static char prefixBuffer[64] = "REMoAINTER_Mat";

        ImGui::Text("Export All Composited PBR Texture Maps:");
        ImGui::Separator();

        ImGui::InputText("Output Folder", dirBuffer, sizeof(dirBuffer));
        ImGui::InputText("Prefix", prefixBuffer, sizeof(prefixBuffer));

        ImGui::BulletText("Base Color (sRGB): %s/%s_BaseColor.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Normal (Tangent):  %s/%s_Normal.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Roughness (Linear):%s/%s_Roughness.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("Metallic (Linear): %s/%s_Metallic.png", dirBuffer, prefixBuffer);
        ImGui::BulletText("glTF Pack (G:R, B:M): %s/%s_MetallicRoughness.png", dirBuffer, prefixBuffer);

        ImGui::Separator();
        if (ImGui::Button("Export Textures", ImVec2(160, 0))) {
            bool success = TextureExporter::exportPBRMaps(layerStack, dirBuffer, prefixBuffer);
            statusMessage = success ? "Textures Exported!" : "Export Failed";
            showExportModal = false;
        }
        ImGui::SameLine();
        if (ImGui::Button("Close", ImVec2(90, 0))) {
            showExportModal = false;
        }

        ImGui::EndPopup();
    }
}

} // namespace REMoAINTER
