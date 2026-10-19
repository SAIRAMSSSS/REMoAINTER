#include "UIManager.hpp"
#include "../io/TextureExporter.hpp"
#include "../../third_party/imgui/imgui_internal.h"
#include <iostream>
#include <cmath>
#include <algorithm>
#include <cstring>

namespace REMoAINTER {

void UIManager::setupDefaultDockLayout(ImGuiID dockspaceId, const ImVec2& workSize) {
    ImGui::DockBuilderRemoveNode(dockspaceId);
    ImGui::DockBuilderAddNode(dockspaceId, ImGuiDockNodeFlags_DockSpace);
    ImGui::DockBuilderSetNodeSize(dockspaceId, workSize);

    ImGuiID dockMain = dockspaceId;
    ImGuiID dockTop = 0;
    ImGuiID dockLeftToolbar = 0;
    ImGuiID dockLeftShelf = 0;
    ImGuiID dockRight = 0;
    ImGuiID dockRightTop = 0;
    ImGuiID dockRightMid = 0;
    ImGuiID dockRightBottom = 0;

    // 1. Top Bar: split top 38px
    float topBarRatio = 38.0f / std::max(1.0f, workSize.y);
    ImGui::DockBuilderSplitNode(dockMain, ImGuiDir_Up, topBarRatio, &dockTop, &dockMain);

    // 2. Far-left narrow toolbar strip (48px)
    float leftToolbarRatio = 48.0f / std::max(1.0f, workSize.x);
    ImGui::DockBuilderSplitNode(dockMain, ImGuiDir_Left, leftToolbarRatio, &dockLeftToolbar, &dockMain);

    // 3. Asset Shelf (280px)
    float shelfRatio = 280.0f / std::max(1.0f, workSize.x - 48.0f);
    ImGui::DockBuilderSplitNode(dockMain, ImGuiDir_Left, shelfRatio, &dockLeftShelf, &dockMain);

    // 4. Right side panel stack (330px)
    float rightRatio = 330.0f / std::max(1.0f, workSize.x - 48.0f - 280.0f);
    ImGui::DockBuilderSplitNode(dockMain, ImGuiDir_Right, rightRatio, &dockRight, &dockMain);

    // 5. Split right side vertically:
    // Top: TEXTURE SET LIST (22%)
    // Mid: LAYERS (44%)
    // Bottom: PROPERTIES - PAINT (34%)
    ImGuiID dockRightRest = 0;
    ImGui::DockBuilderSplitNode(dockRight, ImGuiDir_Up, 0.22f, &dockRightTop, &dockRightRest);
    ImGui::DockBuilderSplitNode(dockRightRest, ImGuiDir_Up, 0.52f, &dockRightMid, &dockRightBottom);

    // 6. Dock windows into designated nodes:
    ImGui::DockBuilderDockWindow("TopBar##ControlBar", dockTop);
    ImGui::DockBuilderDockWindow("Tools##LeftToolbar", dockLeftToolbar);
    ImGui::DockBuilderDockWindow("ASSETS", dockLeftShelf);
    ImGui::DockBuilderDockWindow("TEXTURE SET LIST", dockRightTop);
    ImGui::DockBuilderDockWindow("LAYERS", dockRightMid);
    ImGui::DockBuilderDockWindow("PROPERTIES - PAINT", dockRightBottom);

    ImGui::DockBuilderFinish(dockspaceId);
}

void UIManager::render(LayerStack& layerStack, Brush& brush, const Mesh& mesh, Camera& camera,
                       bool isMeshHovered, const glm::vec2& hoveredUV,
                       ImTextureID logoTexture,
                       std::function<void()> onTriggerComposite) {

    logoTextureID = logoTexture;

    // Setup Main DockSpace Window across entire application (leaving 24px for bottom status bar)
    ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImVec2 dockspaceSize(viewport->WorkSize.x, viewport->WorkSize.y - 24.0f);
    ImGui::SetNextWindowPos(viewport->WorkPos);
    ImGui::SetNextWindowSize(dockspaceSize);
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

    // Initialize or reset dock layout programmatically
    ImGuiDockNode* node = ImGui::DockBuilderGetNode(dockspaceId);
    if (node == nullptr || node->IsEmpty() || requestLayoutReset) {
        setupDefaultDockLayout(dockspaceId, dockspaceSize);
        requestLayoutReset = false;
    }

    ImGui::DockSpace(dockspaceId, ImVec2(0.0f, 0.0f), ImGuiDockNodeFlags_PassthruCentralNode);

    renderMainMenuBar(layerStack);
    renderTopControlBar(brush);
    renderLeftToolbar(brush);
    renderAssetShelf(layerStack, brush, onTriggerComposite);
    renderTextureSetList(layerStack);
    renderLayerStack(layerStack, brush, onTriggerComposite);
    renderPropertiesPanel(layerStack, brush, onTriggerComposite);
    renderStatusBar();
    renderExportDialog(layerStack);
    renderAboutDialog();

    ImGui::End();
}

void UIManager::renderMainMenuBar(LayerStack& layerStack) {
    if (ImGui::BeginMenuBar()) {
        // Authentic REMoAINTER Brand Badge with Logo
        if (logoTextureID) {
            ImGui::Image(logoTextureID, ImVec2(20, 20));
            ImGui::SameLine();
        } else {
            ImDrawList* menuDraw = ImGui::GetWindowDrawList();
            ImVec2 p = ImGui::GetCursorScreenPos();
            menuDraw->AddRectFilled(ImVec2(p.x, p.y + 2), ImVec2(p.x + 20, p.y + 20), IM_COL32(235, 115, 20, 255), 3.0f);
            menuDraw->AddText(ImVec2(p.x + 4, p.y + 3), IM_COL32(255, 255, 255, 255), "Pt");
            ImGui::Dummy(ImVec2(24, 20));
            ImGui::SameLine();
        }

        if (ImGui::BeginMenu("File")) {
            if (ImGui::MenuItem("New Project...", "Ctrl+N")) {
                layerStack.layers.clear();
                layerStack.addFillLayer("Base Material", glm::vec4(0.82f, 0.82f, 0.84f, 1.0f));
                layerStack.addPaintLayer("Layer 1");
                layerStack.composite();
                statusMessage = "Created New Project";
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
            if (ImGui::MenuItem("Undo", "Ctrl+Z")) statusMessage = "Undo performed";
            if (ImGui::MenuItem("Redo", "Ctrl+Y")) statusMessage = "Redo performed";
            ImGui::Separator();
            if (ImGui::MenuItem("Clear Active Layer")) {
                auto active = layerStack.getActiveLayer();
                if (active) {
                    active->clear();
                    layerStack.composite();
                    statusMessage = "Cleared active layer";
                }
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Mode")) {
            if (ImGui::MenuItem("Paint Mode", "1", activeTool == ToolMode::Brush)) activeTool = ToolMode::Brush;
            if (ImGui::MenuItem("Eraser Mode", "2", activeTool == ToolMode::Eraser)) activeTool = ToolMode::Eraser;
            if (ImGui::MenuItem("Fill Mode", "4", activeTool == ToolMode::PolygonFill)) activeTool = ToolMode::PolygonFill;
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Window")) {
            if (ImGui::MenuItem("Reset Layout to Default", "Shift+Alt+R")) {
                requestLayoutReset = true;
                statusMessage = "Layout reset to default";
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Viewport")) {
            if (ImGui::MenuItem("Material (Lit)", "M", displayChannel == 0)) displayChannel = 0;
            if (ImGui::MenuItem("Base Color (Solo)", "C", displayChannel == 1)) displayChannel = 1;
            if (ImGui::MenuItem("Roughness (Solo)", nullptr, displayChannel == 2)) displayChannel = 2;
            if (ImGui::MenuItem("Metallic (Solo)", nullptr, displayChannel == 3)) displayChannel = 3;
            if (ImGui::MenuItem("Normal (Solo)", nullptr, displayChannel == 4)) displayChannel = 4;
            if (ImGui::MenuItem("Lighting Only", nullptr, displayChannel == 5)) displayChannel = 5;
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Geometry")) {
            if (ImGui::MenuItem("Substance Material Sphere", nullptr, requestedMeshType == 0)) {
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
            ImGui::InputText("##objPath", objPathBuffer, IM_ARRAYSIZE(objPathBuffer));
            if (ImGui::Button("Load OBJ File") && strlen(objPathBuffer) > 0) {
                requestedOBJPath = objPathBuffer;
                requestedMeshType = 3;
                requestMeshChange = true;
            }
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Python")) {
            ImGui::MenuItem("Script Editor");
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("JavaScript")) {
            ImGui::MenuItem("Plugins Console");
            ImGui::EndMenu();
        }

        if (ImGui::BeginMenu("Help")) {
            if (ImGui::MenuItem("About REMoAINTER...")) {
                showAboutModal = true;
            }
            ImGui::EndMenu();
        }

        // Top-right brand watermark
        ImGui::SameLine(ImGui::GetWindowWidth() - 280);
        ImGui::TextColored(ImVec4(0.95f, 0.48f, 0.15f, 1.0f), "REMoAINTER");
        ImGui::SameLine();
        ImGui::TextDisabled("| %s", statusMessage.c_str());

        ImGui::EndMenuBar();
    }
}

void UIManager::renderTopControlBar(Brush& brush) {
    ImGuiWindowClass window_class;
    window_class.DockNodeFlagsOverrideSet = ImGuiDockNodeFlags_NoTabBar | ImGuiDockNodeFlags_NoCloseButton;
    ImGui::SetNextWindowClass(&window_class);

    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(8, 4));
    ImGui::Begin("TopBar##ControlBar", nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoScrollbar);

    // Quick Undo / Redo
    if (ImGui::Button("Undo")) statusMessage = "Undo";
    ImGui::SameLine();
    if (ImGui::Button("Redo")) statusMessage = "Redo";
    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Size Slider + Pen Pressure Toggle
    ImGui::Text("Size");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(80);
    ImGui::SliderFloat("##topsize", &brush.size, 1.0f, 128.0f, "%.0f");
    ImGui::SameLine();
    if (ImGui::Button(brush.penPressure ? "[P:ON]" : "[P:OFF]")) {
        brush.penPressure = !brush.penPressure;
    }
    if (ImGui::IsItemHovered()) ImGui::SetTooltip("Simulate Pressure Sensitivity");

    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Flow
    ImGui::Text("Flow");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(70);
    float flowPercent = brush.flow * 100.0f;
    if (ImGui::SliderFloat("##topflow", &flowPercent, 1.0f, 100.0f, "%.0f")) {
        brush.flow = flowPercent / 100.0f;
    }

    ImGui::SameLine();
    ImGui::Text("Stroke opacity");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(70);
    float opPercent = brush.opacity * 100.0f;
    if (ImGui::SliderFloat("##topop", &opPercent, 1.0f, 100.0f, "%.0f")) {
        brush.opacity = opPercent / 100.0f;
    }

    ImGui::SameLine();
    ImGui::Text("Spacing");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(65);
    if (ImGui::SliderFloat("##topspacing", &strokeSpacing, 1.0f, 100.0f, "%.0f")) {
        brush.spacing = strokeSpacing / 100.0f;
    }

    ImGui::SameLine();
    ImGui::Text("Angle");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(65);
    float angleDeg = brush.angle * (180.0f / 3.14159265f);
    if (ImGui::SliderFloat("##topangle", &angleDeg, 0.0f, 360.0f, "%.0f deg")) {
        brush.angle = angleDeg * (3.14159265f / 180.0f);
    }

    ImGui::SameLine();
    ImGui::TextDisabled("|");
    ImGui::SameLine();

    // Symmetry & Lazy Mouse
    if (ImGui::Button(symmetryX ? "[Symmetry: ON]" : "[Symmetry: OFF]")) {
        symmetryX = !symmetryX;
    }
    ImGui::SameLine();
    if (ImGui::Button(brush.lazyMouse ? "[LazyMouse: ON]" : "[LazyMouse: OFF]")) {
        brush.lazyMouse = !brush.lazyMouse;
    }
    if (ImGui::IsItemHovered()) ImGui::SetTooltip("Smooth Stroke / Lazy Mouse Interpolation");

    // Right side: Lighting Preset & Viewport Mode dropdown
    float rightWidth = 330.0f;
    float targetX = ImGui::GetWindowWidth() - rightWidth;
    if (ImGui::GetCursorPosX() < targetX) {
        ImGui::SameLine(targetX);
    } else {
        ImGui::SameLine();
    }
    ImGui::Text("Lighting:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(115);
    const char* envs[] = { "Studio Neutral", "Warm & Cool", "Cyberpunk", "Forest", "Desert" };
    ImGui::Combo("##topEnv", &environmentPreset, envs, IM_ARRAYSIZE(envs));

    ImGui::SameLine();
    ImGui::SetNextItemWidth(105);
    const char* channels[] = { "Material", "Base Color", "Roughness", "Metallic", "Normal", "Lighting Only" };
    ImGui::Combo("##topChannel", &displayChannel, channels, IM_ARRAYSIZE(channels));

    ImGui::End();
    ImGui::PopStyleVar();
}

void UIManager::drawToolIcon(ImDrawList* drawList, const ImVec2& pMin, const ImVec2& pMax, ToolMode mode, bool active) {
    float cx = (pMin.x + pMax.x) * 0.5f;
    float cy = (pMin.y + pMax.y) * 0.5f;

    ImU32 iconColor = active ? IM_COL32(255, 255, 255, 255) : IM_COL32(210, 215, 225, 255);
    ImU32 accentColor = active ? IM_COL32(255, 220, 100, 255) : IM_COL32(235, 120, 30, 255);

    switch (mode) {
        case ToolMode::Brush: {
            // Paint Brush: wooden handle, metal ferrule, bristle tip
            drawList->AddLine(ImVec2(cx + 9, cy - 9), ImVec2(cx - 1, cy + 1), IM_COL32(215, 175, 120, 255), 3.0f);
            drawList->AddLine(ImVec2(cx - 1, cy + 1), ImVec2(cx - 4, cy + 4), IM_COL32(190, 195, 205, 255), 3.5f);
            drawList->AddTriangleFilled(ImVec2(cx - 3, cy + 6), ImVec2(cx - 6, cy + 3), ImVec2(cx - 10, cy + 10), accentColor);
            break;
        }
        case ToolMode::Eraser: {
            // Eraser: angled two-tone rubber block
            ImVec2 pts[4] = {
                ImVec2(cx - 7, cy - 4),
                ImVec2(cx + 3, cy - 9),
                ImVec2(cx + 8, cy + 3),
                ImVec2(cx - 2, cy + 8)
            };
            drawList->AddConvexPolyFilled(pts, 4, IM_COL32(230, 140, 160, 255));
            drawList->AddLine(ImVec2(cx, cy - 1), ImVec2(cx + 3, cy + 5), IM_COL32(255, 255, 255, 200), 1.5f);
            drawList->AddPolyline(pts, 4, iconColor, ImDrawFlags_Closed, 1.2f);
            break;
        }
        case ToolMode::Projection: {
            // Camera projector icon with projection frustum
            drawList->AddRectFilled(ImVec2(cx - 8, cy - 5), ImVec2(cx + 2, cy + 5), iconColor, 2.0f);
            drawList->AddCircleFilled(ImVec2(cx - 3, cy), 2.5f, IM_COL32(40, 44, 52, 255));
            drawList->AddTriangleFilled(ImVec2(cx + 2, cy - 2), ImVec2(cx + 8, cy - 7), ImVec2(cx + 8, cy + 7), accentColor);
            break;
        }
        case ToolMode::PolygonFill: {
            // Paint bucket with falling paint drop
            ImVec2 bPts[4] = {
                ImVec2(cx - 6, cy - 6),
                ImVec2(cx + 2, cy - 8),
                ImVec2(cx + 7, cy + 4),
                ImVec2(cx - 1, cy + 6)
            };
            drawList->AddConvexPolyFilled(bPts, 4, iconColor);
            drawList->AddLine(ImVec2(cx - 5, cy - 5), ImVec2(cx + 2, cy - 7), accentColor, 2.5f);
            drawList->AddCircleFilled(ImVec2(cx + 5, cy + 7), 2.5f, accentColor);
            break;
        }
        case ToolMode::Smudge: {
            // Fluid smudge swirl
            drawList->AddBezierCubic(ImVec2(cx - 8, cy + 7), ImVec2(cx - 4, cy - 8),
                                     ImVec2(cx + 4, cy + 8), ImVec2(cx + 8, cy - 6),
                                     accentColor, 2.8f);
            drawList->AddCircleFilled(ImVec2(cx + 8, cy - 6), 2.0f, iconColor);
            break;
        }
        case ToolMode::Clone: {
            // Rubber Stamp: handle knob and base pad
            drawList->AddCircleFilled(ImVec2(cx, cy - 6), 3.5f, iconColor);
            drawList->AddLine(ImVec2(cx, cy - 3), ImVec2(cx, cy + 3), iconColor, 2.5f);
            drawList->AddRectFilled(ImVec2(cx - 8, cy + 3), ImVec2(cx + 8, cy + 8), accentColor, 1.5f);
            break;
        }
        case ToolMode::Eyedropper: {
            // Pipette with rubber bulb
            drawList->AddCircleFilled(ImVec2(cx + 6, cy - 6), 3.5f, accentColor);
            drawList->AddLine(ImVec2(cx + 4, cy - 4), ImVec2(cx - 6, cy + 6), iconColor, 2.2f);
            drawList->AddLine(ImVec2(cx - 6, cy + 6), ImVec2(cx - 9, cy + 9), accentColor, 1.5f);
            break;
        }
    }
}

void UIManager::drawToolButton(ToolMode mode, const char* id, const char* label, const char* tooltip, const char* shortcut, bool active, std::function<void()> onClick) {
    ImVec2 btnSize(38, 38);
    ImDrawList* drawList = ImGui::GetWindowDrawList();

    if (active) {
        ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        ImGui::PushStyleColor(ImGuiCol_ButtonHovered, ImVec4(0.20f, 0.52f, 0.92f, 1.0f));
    } else {
        ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.20f, 0.22f, 0.26f, 1.0f));
        ImGui::PushStyleColor(ImGuiCol_ButtonHovered, ImVec4(0.28f, 0.31f, 0.36f, 1.0f));
    }

    ImVec2 pMin = ImGui::GetCursorScreenPos();
    ImVec2 pMax = ImVec2(pMin.x + btnSize.x, pMin.y + btnSize.y);

    if (ImGui::Button(id, btnSize)) {
        if (onClick) onClick();
    }

    ImGui::PopStyleColor(2);

    // Draw vector icon centered inside the button
    drawToolIcon(drawList, pMin, pMax, mode, active);

    if (ImGui::IsItemHovered()) {
        if (shortcut && strlen(shortcut) > 0) {
            ImGui::SetTooltip("%s (%s)", tooltip, shortcut);
        } else {
            ImGui::SetTooltip("%s", tooltip);
        }
    }
}

void UIManager::renderLeftToolbar(Brush& brush) {
    ImGuiWindowClass window_class;
    window_class.DockNodeFlagsOverrideSet = ImGuiDockNodeFlags_NoTabBar | ImGuiDockNodeFlags_NoCloseButton;
    ImGui::SetNextWindowClass(&window_class);

    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(4, 6));
    ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, ImVec2(4, 6));
    ImGui::Begin("Tools##LeftToolbar", nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoCollapse);

    // 1. Paint Tool
    bool isPaint = (activeTool == ToolMode::Brush && !brush.isEraser);
    drawToolButton(ToolMode::Brush, "##btnPaint", "Paint", "Paint Brush Tool", "1", isPaint, [&]() {
        activeTool = ToolMode::Brush;
        brush.isEraser = false;
        statusMessage = "Paint Tool Active (1)";
    });

    // 2. Eraser Tool
    bool isEraser = (activeTool == ToolMode::Eraser || brush.isEraser);
    drawToolButton(ToolMode::Eraser, "##btnEraser", "Eraser", "Eraser Tool", "2", isEraser, [&]() {
        activeTool = ToolMode::Eraser;
        brush.isEraser = true;
        statusMessage = "Eraser Tool Active (2)";
    });

    // 3. Projection Tool
    drawToolButton(ToolMode::Projection, "##btnProj", "Projection", "Projection Painting Tool", "3", activeTool == ToolMode::Projection, [&]() {
        activeTool = ToolMode::Projection;
        statusMessage = "Projection Tool Active (3)";
    });

    // 4. Polygon Fill Tool
    drawToolButton(ToolMode::PolygonFill, "##btnFill", "Fill", "Polygon Fill Tool", "4 / F", activeTool == ToolMode::PolygonFill, [&]() {
        activeTool = ToolMode::PolygonFill;
        statusMessage = "Polygon Fill Tool Active (4 / F)";
    });

    // 5. Smudge Tool
    drawToolButton(ToolMode::Smudge, "##btnSmudge", "Smudge", "Smudge Blending Tool", "5", activeTool == ToolMode::Smudge, [&]() {
        activeTool = ToolMode::Smudge;
        statusMessage = "Smudge Tool Active (5)";
    });

    // 6. Clone Stamp Tool
    drawToolButton(ToolMode::Clone, "##btnClone", "Clone", "Clone Stamp Tool", "6", activeTool == ToolMode::Clone, [&]() {
        activeTool = ToolMode::Clone;
        statusMessage = "Clone Tool Active (6)";
    });

    // 7. Eyedropper Tool
    drawToolButton(ToolMode::Eyedropper, "##btnPick", "Pick", "Material & Color Eyedropper", "I", activeTool == ToolMode::Eyedropper, [&]() {
        activeTool = ToolMode::Eyedropper;
        statusMessage = "Eyedropper Tool Active (I)";
    });

    ImGui::Separator();

    // Symmetry Button
    ImDrawList* drawList = ImGui::GetWindowDrawList();
    ImVec2 pMin = ImGui::GetCursorScreenPos();
    ImVec2 pMax = ImVec2(pMin.x + 38, pMin.y + 38);

    if (symmetryX) {
        ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
    } else {
        ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.20f, 0.22f, 0.26f, 1.0f));
    }
    if (ImGui::Button("##btnSymmetry", ImVec2(38, 38))) {
        symmetryX = !symmetryX;
        statusMessage = symmetryX ? "Symmetry Enabled" : "Symmetry Disabled";
    }
    ImGui::PopStyleColor();

    float scx = (pMin.x + pMax.x) * 0.5f;
    float scy = (pMin.y + pMax.y) * 0.5f;
    drawList->AddLine(ImVec2(scx, scy - 8), ImVec2(scx, scy + 8), IM_COL32(255, 255, 255, 255), 1.5f);
    drawList->AddTriangleFilled(ImVec2(scx - 2, scy), ImVec2(scx - 7, scy - 4), ImVec2(scx - 7, scy + 4), IM_COL32(235, 120, 30, 255));
    drawList->AddTriangleFilled(ImVec2(scx + 2, scy), ImVec2(scx + 7, scy - 4), ImVec2(scx + 7, scy + 4), IM_COL32(235, 120, 30, 255));

    if (ImGui::IsItemHovered()) {
        ImGui::SetTooltip("X-Axis Symmetry (%s)", symmetryX ? "ON" : "OFF");
    }

    ImGui::End();
    ImGui::PopStyleVar(2);
}

void UIManager::renderAssetShelf(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("ASSETS", nullptr);

    // Header filter pills
    const char* filters[] = { "All", "Smart Mats", "Materials", "Alphas", "Brushes", "Envs" };
    for (int i = 0; i < 6; ++i) {
        if (i > 0) ImGui::SameLine();
        bool active = (assetFilterCategory == i);
        if (active) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button(filters[i])) {
            assetFilterCategory = i;
        }
        if (active) ImGui::PopStyleColor();
    }

    ImGui::Spacing();

    // Search bar
    ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x - 100);
    ImGui::InputTextWithHint("##assetSearch", "Search...", assetSearchBuffer, IM_ARRAYSIZE(assetSearchBuffer));
    ImGui::SameLine();
    ImGui::SetNextItemWidth(90);
    const char* libs[] = { "All libs", "Starter", "User" };
    static int currentLib = 0;
    ImGui::Combo("##libs", &currentLib, libs, IM_ARRAYSIZE(libs));

    ImGui::Separator();

    if (assetFilterCategory == 1) {
        // Smart Materials: Instantiate full compound layer hierarchies
        auto smartMats = PresetLibrary::getSmartMaterials();
        for (const auto& sm : smartMats) {
            if (strlen(assetSearchBuffer) > 0) {
                std::string nameLower = sm.name;
                std::string searchLower = assetSearchBuffer;
                std::transform(nameLower.begin(), nameLower.end(), nameLower.begin(), ::tolower);
                std::transform(searchLower.begin(), searchLower.end(), searchLower.begin(), ::tolower);
                if (nameLower.find(searchLower) == std::string::npos) continue;
            }

            ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.20f, 0.22f, 0.26f, 1.0f));
            std::string headerText = "[SM] " + sm.name + " (" + std::to_string(sm.layers.size()) + " layers)";
            if (ImGui::Button(headerText.c_str(), ImVec2(-1, 32))) {
                layerStack.layers.clear();
                for (const auto& lDef : sm.layers) {
                    if (lDef.isFill) {
                        layerStack.addFillLayer(lDef.name, lDef.color);
                        auto act = layerStack.getActiveLayer();
                        if (act) {
                            act->fillMetallic = lDef.metallic;
                            act->fillRoughness = lDef.roughness;
                            act->opacity = lDef.opacity;
                            if (lDef.blendMode == "Mult") act->blendMode = BlendMode::Multiply;
                            else if (lDef.blendMode == "Screen") act->blendMode = BlendMode::Screen;
                            else if (lDef.blendMode == "Over") act->blendMode = BlendMode::Overlay;
                            else if (lDef.blendMode == "Add") act->blendMode = BlendMode::LinearAdd;
                            else act->blendMode = BlendMode::Normal;
                        }
                    } else {
                        layerStack.addPaintLayer(lDef.name);
                        auto act = layerStack.getActiveLayer();
                        if (act) {
                            act->opacity = lDef.opacity;
                        }
                    }
                }
                layerStack.composite();
                if (onTriggerComposite) onTriggerComposite();
                statusMessage = "Loaded Smart Material: " + sm.name;
            }
            ImGui::PopStyleColor();
            if (ImGui::IsItemHovered()) {
                ImGui::SetTooltip("Category: %s\nClick to instantiate authentic %zu-layer compound hierarchy",
                                  sm.category.c_str(), sm.layers.size());
            }
            ImGui::Spacing();
        }
    } else if (assetFilterCategory == 3) {
        // Alphas & Decals & Logos
        auto alphas = PresetLibrary::getAlphas();
        for (const auto& a : alphas) {
            if (strlen(assetSearchBuffer) > 0) {
                std::string nameLower = a.name;
                std::string searchLower = assetSearchBuffer;
                std::transform(nameLower.begin(), nameLower.end(), nameLower.begin(), ::tolower);
                std::transform(searchLower.begin(), searchLower.end(), searchLower.begin(), ::tolower);
                if (nameLower.find(searchLower) == std::string::npos) continue;
            }

            bool active = (brush.alphaType == a.type);
            if (active) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
            std::string btnText = "[" + a.category + "] " + a.name;
            if (ImGui::Button(btnText.c_str(), ImVec2(-1, 28))) {
                brush.alphaType = a.type;
                brush.alphaName = a.name;
                statusMessage = "Loaded Alpha Mask: " + a.name;
            }
            if (active) ImGui::PopStyleColor();
        }
    } else if (assetFilterCategory == 4) {
        // Brushes
        auto brushes = PresetLibrary::getBrushes();
        for (const auto& b : brushes) {
            std::string bText = b.name + " (" + std::to_string((int)b.size) + "px)";
            if (ImGui::Button(bText.c_str(), ImVec2(-1, 28))) {
                brush.size = b.size;
                brush.hardness = b.hardness;
                brush.opacity = b.opacity;
                brush.flow = b.flow;
                brush.spacing = b.spacing;
                brush.alphaType = b.alphaIndex;
                statusMessage = "Loaded Brush Preset: " + b.name;
            }
        }
    } else if (assetFilterCategory == 5) {
        // Environments
        auto envs = PresetLibrary::getEnvironments();
        for (const auto& e : envs) {
            bool active = (environmentPreset == e.id);
            if (active) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
            if (ImGui::Button(e.name.c_str(), ImVec2(-1, 28))) {
                environmentPreset = e.id;
                statusMessage = "Active Environment: " + e.name;
            }
            if (active) ImGui::PopStyleColor();
            if (ImGui::IsItemHovered()) ImGui::SetTooltip("%s", e.description.c_str());
        }
    } else {
        // All or Single Materials
        auto materials = PresetLibrary::getMaterials();
        ImDrawList* drawList = ImGui::GetWindowDrawList();

        float panelWidth = ImGui::GetContentRegionAvail().x;
        int columns = std::max(1, (int)(panelWidth / 82.0f));
        float itemWidth = panelWidth / (float)columns;
        float sphereRadius = 24.0f;

        int itemIndex = 0;
        for (const auto& mat : materials) {
            if (strlen(assetSearchBuffer) > 0) {
                std::string nameLower = mat.name;
                std::string searchLower = assetSearchBuffer;
                std::transform(nameLower.begin(), nameLower.end(), nameLower.begin(), ::tolower);
                std::transform(searchLower.begin(), searchLower.end(), searchLower.begin(), ::tolower);
                if (nameLower.find(searchLower) == std::string::npos) continue;
            }

            if (itemIndex % columns != 0) {
                ImGui::SameLine();
            }

            ImGui::BeginGroup();

            ImVec2 curPos = ImGui::GetCursorScreenPos();
            ImVec2 sphereCenter(curPos.x + itemWidth * 0.5f, curPos.y + sphereRadius + 4.0f);

            ImGui::InvisibleButton(mat.name.c_str(), ImVec2(itemWidth - 4.0f, sphereRadius * 2.0f + 24.0f));
            bool isHovered = ImGui::IsItemHovered();
            bool isClicked = ImGui::IsItemClicked();

            drawShelfMaterialThumbnail(drawList, sphereCenter, sphereRadius, mat.baseColor, mat.metallic, mat.roughness);

            if (isHovered) {
                drawList->AddCircle(sphereCenter, sphereRadius + 2.0f, IM_COL32(36, 112, 209, 255), 0, 2.0f);
                ImGui::SetTooltip("%s\nCategory: %s\nMetallic: %.2f  Roughness: %.2f",
                                  mat.name.c_str(), mat.category.c_str(), mat.metallic, mat.roughness);
            }

            if (isClicked) {
                brush.color = mat.baseColor;
                brush.metallic = mat.metallic;
                brush.roughness = mat.roughness;
                brush.paintColor = true;
                brush.paintMetallic = true;
                brush.paintRoughness = true;

                auto active = layerStack.getActiveLayer();
                if (active && active->type == LayerType::Fill) {
                    active->fillColor = mat.baseColor;
                    active->fillMetallic = mat.metallic;
                    active->fillRoughness = mat.roughness;
                    layerStack.composite();
                    if (onTriggerComposite) onTriggerComposite();
                }

                statusMessage = "Loaded Material: " + mat.name;
            }

            std::string label = mat.name;
            if (label.length() > 9) label = label.substr(0, 8) + "...";
            float textWidth = ImGui::CalcTextSize(label.c_str()).x;
            ImGui::SetCursorScreenPos(ImVec2(curPos.x + (itemWidth - textWidth) * 0.5f, curPos.y + sphereRadius * 2.0f + 6.0f));
            ImGui::TextDisabled("%s", label.c_str());

            ImGui::EndGroup();
            itemIndex++;
        }
    }

    ImGui::End();
}

void UIManager::renderTextureSetList(LayerStack& layerStack) {
    ImGui::Begin("TEXTURE SET LIST", nullptr);

    ImGui::TextColored(ImVec4(0.85f, 0.55f, 0.20f, 1.0f), "Texture Sets");
    ImGui::SameLine(ImGui::GetWindowWidth() - 30);
    ImGui::TextDisabled("[*]");

    // Active Texture Set (Selected with Blue highlight matching screenshot)
    ImGui::PushStyleColor(ImGuiCol_Header, ImVec4(0.14f, 0.44f, 0.82f, 0.85f));
    ImGui::Selectable(textureSetName.c_str(), true);
    ImGui::PopStyleColor();

    ImGui::Spacing();
    ImGui::Separator();
    ImGui::Spacing();

    // Resolution Selector
    ImGui::Text("Size:");
    ImGui::SameLine();
    ImGui::SetNextItemWidth(120);
    const char* resolutions[] = { "512", "1024", "2048", "4096" };
    int currentResIdx = 2; // 2048
    if (textureResolution == 512) currentResIdx = 0;
    else if (textureResolution == 1024) currentResIdx = 1;
    else if (textureResolution == 2048) currentResIdx = 2;
    else if (textureResolution == 4096) currentResIdx = 3;

    if (ImGui::Combo("##texSetRes", &currentResIdx, resolutions, IM_ARRAYSIZE(resolutions))) {
        if (currentResIdx == 0) textureResolution = 512;
        else if (currentResIdx == 1) textureResolution = 1024;
        else if (currentResIdx == 2) textureResolution = 2048;
        else if (currentResIdx == 3) textureResolution = 4096;
        layerStack.resize(textureResolution, textureResolution);
        statusMessage = "Texture Set Resized to " + std::to_string(textureResolution);
    }

    ImGui::End();
}

void UIManager::renderLayerStack(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("LAYERS", nullptr);

    // Tab bar: LAYERS (Active with blue bar) | TEXTURE SET SETTINGS
    if (ImGui::BeginTabBar("##LayerTabs")) {
        if (ImGui::BeginTabItem("LAYERS")) {

            // Channel selector dropdown (Base color, Metallic, Roughness, Normal)
            ImGui::SetNextItemWidth(110);
            const char* channels[] = { "Base color", "Metallic", "Roughness", "Normal" };
            ImGui::Combo("##layerChanView", &layerChannelView, channels, IM_ARRAYSIZE(channels));

            ImGui::SameLine();
            auto activeLayer = layerStack.getActiveLayer();
            if (activeLayer) {
                ImGui::SetNextItemWidth(75);
                const char* blendNames[] = { "Norm", "Mult", "Screen", "Over", "Add" };
                int currentBlend = static_cast<int>(activeLayer->blendMode);
                if (ImGui::Combo("##blendMode", &currentBlend, blendNames, IM_ARRAYSIZE(blendNames))) {
                    activeLayer->blendMode = static_cast<BlendMode>(currentBlend);
                    layerStack.composite();
                    if (onTriggerComposite) onTriggerComposite();
                }

                ImGui::SameLine();
                ImGui::SetNextItemWidth(70);
                float opPercent = activeLayer->opacity * 100.0f;
                if (ImGui::SliderFloat("##layerOp", &opPercent, 0.0f, 100.0f, "%.0f%%")) {
                    activeLayer->opacity = opPercent / 100.0f;
                    layerStack.composite();
                    if (onTriggerComposite) onTriggerComposite();
                }
            }

            ImGui::Separator();

            // Layer Action Toolbar row: [+ Paint] [+ Fill] [+ Folder] [+ Mask] [Del]
            if (ImGui::Button("+ Paint")) {
                layerStack.addPaintLayer("Layer " + std::to_string(layerStack.layers.size() + 1));
                layerStack.composite();
                if (onTriggerComposite) onTriggerComposite();
                statusMessage = "Added Paint Layer";
            }
            ImGui::SameLine();
            if (ImGui::Button("+ Fill")) {
                layerStack.addFillLayer("Fill " + std::to_string(layerStack.layers.size() + 1), brush.color);
                layerStack.composite();
                if (onTriggerComposite) onTriggerComposite();
                statusMessage = "Added Fill Layer";
            }
            ImGui::SameLine();
            if (ImGui::Button("+ Folder")) {
                statusMessage = "Added Folder";
            }
            ImGui::SameLine();
            if (ImGui::Button("+ Mask")) {
                statusMessage = "Added Mask";
            }
            ImGui::SameLine(ImGui::GetWindowWidth() - 45);
            if (ImGui::Button("Del")) {
                if (layerStack.layers.size() > 1) {
                    layerStack.removeLayer(layerStack.activeLayerIndex);
                    layerStack.composite();
                    if (onTriggerComposite) onTriggerComposite();
                    statusMessage = "Deleted Layer";
                }
            }

            ImGui::Separator();

            // Hierarchical Layer Tree matching screenshot
            ImGui::BeginChild("##LayerListRegion", ImVec2(0, 0), true);

            for (int i = (int)layerStack.layers.size() - 1; i >= 0; --i) {
                auto& layer = layerStack.layers[i];
                bool isSelected = (layerStack.activeLayerIndex == (size_t)i);

                ImGui::PushID(i);

                // Visibility Eye Toggle
                if (ImGui::Button(layer->visible ? "[O]" : "[-]")) {
                    layer->visible = !layer->visible;
                    layerStack.composite();
                    if (onTriggerComposite) onTriggerComposite();
                }
                ImGui::SameLine();

                // Swatch preview square
                ImVec4 swatchCol = (layer->type == LayerType::Fill)
                    ? ImVec4(layer->fillColor.r, layer->fillColor.g, layer->fillColor.b, 1.0f)
                    : ImVec4(0.24f, 0.45f, 0.65f, 1.0f);
                ImGui::ColorButton("##swatch", swatchCol, ImGuiColorEditFlags_NoTooltip | ImGuiColorEditFlags_NoBorder, ImVec2(16, 16));
                ImGui::SameLine();

                // Layer row selectable
                std::string itemText = layer->name + ((layer->type == LayerType::Fill) ? " (Fill)" : " (Paint)");
                if (isSelected) ImGui::PushStyleColor(ImGuiCol_Header, ImVec4(0.14f, 0.44f, 0.82f, 0.85f));
                if (ImGui::Selectable(itemText.c_str(), isSelected, ImGuiSelectableFlags_SpanAllColumns)) {
                    layerStack.activeLayerIndex = (size_t)i;
                }
                if (isSelected) ImGui::PopStyleColor();

                ImGui::PopID();
            }

            ImGui::EndChild();
            ImGui::EndTabItem();
        }

        if (ImGui::BeginTabItem("TEXTURE SET SETTINGS")) {
            ImGui::Text("Shader: PBR Metal/Roughness");
            ImGui::Text("Normal Map Format: DirectX / OpenGL");
            ImGui::Text("Displacement: Parallax Occlusion");
            ImGui::EndTabItem();
        }

        ImGui::EndTabBar();
    }

    ImGui::End();
}

void UIManager::renderPropertiesPanel(LayerStack& layerStack, Brush& brush, std::function<void()> onTriggerComposite) {
    ImGui::Begin("PROPERTIES - PAINT", nullptr);

    // Two Large Side-by-Side Preview Boxes:
    // Left: Stroke Preview (wavy white stroke on dark square)
    // Right: 3D Material Preview Sphere
    float boxWidth = (ImGui::GetContentRegionAvail().x - 12.0f) * 0.5f;
    float boxHeight = 100.0f;
    ImDrawList* drawList = ImGui::GetWindowDrawList();

    ImVec2 pos1 = ImGui::GetCursorScreenPos();
    ImGui::InvisibleButton("##strokeBox", ImVec2(boxWidth, boxHeight));
    drawStrokePreview(drawList, pos1, ImVec2(pos1.x + boxWidth, pos1.y + boxHeight), brush);

    ImGui::SameLine();

    ImVec2 pos2 = ImGui::GetCursorScreenPos();
    ImGui::InvisibleButton("##materialBox", ImVec2(boxWidth, boxHeight));
    drawMaterialSpherePreview(drawList, pos2, ImVec2(pos2.x + boxWidth, pos2.y + boxHeight), brush.color, brush.metallic, brush.roughness);

    ImGui::Spacing();

    // Mode icons row: [Brush] [Alpha] [Stencil] [Material]
    const char* pTabs[] = { "BRUSH", "ALPHA", "STENCIL", "MATERIAL" };
    for (int t = 0; t < 4; ++t) {
        if (t > 0) ImGui::SameLine();
        bool active = (activePropertiesTab == t);
        if (active) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button(pTabs[t])) {
            activePropertiesTab = t;
        }
        if (active) ImGui::PopStyleColor();
    }

    ImGui::Separator();

    // 1. BRUSH Section
    if (ImGui::CollapsingHeader("BRUSH", ImGuiTreeNodeFlags_DefaultOpen)) {
        ImGui::SliderFloat("Size", &brush.size, 1.0f, 128.0f, "%.0f px");
        ImGui::SliderFloat("Minimum Size (%)", &brush.minSizePercent, 1.0f, 100.0f, "%.0f%%");

        float flowPct = brush.flow * 100.0f;
        if (ImGui::SliderFloat("Flow", &flowPct, 1.0f, 100.0f, "%.0f%%")) {
            brush.flow = flowPct / 100.0f;
        }

        float opPct = brush.opacity * 100.0f;
        if (ImGui::SliderFloat("Stroke opacity", &opPct, 1.0f, 100.0f, "%.0f%%")) {
            brush.opacity = opPct / 100.0f;
        }

        if (ImGui::SliderFloat("Spacing", &strokeSpacing, 1.0f, 100.0f, "%.0f%%")) {
            brush.spacing = strokeSpacing / 100.0f;
        }

        float deg = brush.angle * (180.0f / 3.14159265f);
        if (ImGui::SliderFloat("Angle", &deg, 0.0f, 360.0f, "%.0f deg")) {
            brush.angle = deg * (3.14159265f / 180.0f);
        }

        ImGui::Checkbox("Follow path", &brush.followPath);
        ImGui::SameLine();
        ImGui::Checkbox("Lazy Mouse", &brush.lazyMouse);
    }

    // 2. ALPHA Section
    if (ImGui::CollapsingHeader("ALPHA", ImGuiTreeNodeFlags_DefaultOpen)) {
        ImGui::Text("Active Mask: %s", brush.alphaName.c_str());

        auto alphas = PresetLibrary::getAlphas();
        for (size_t i = 0; i < alphas.size(); ++i) {
            if (i > 0 && i % 3 != 0) ImGui::SameLine();
            bool selected = (brush.alphaType == alphas[i].type);
            if (selected) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
            if (ImGui::Button(alphas[i].name.c_str())) {
                brush.alphaType = alphas[i].type;
                brush.alphaName = alphas[i].name;
            }
            if (selected) ImGui::PopStyleColor();
        }
    }

    // 3. MATERIAL Section
    if (ImGui::CollapsingHeader("MATERIAL", ImGuiTreeNodeFlags_DefaultOpen)) {
        // Channel Buttons: [Color] [Metal] [Rough] [Nrm] [Height]
        bool c = brush.paintColor;
        if (c) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button("Color")) brush.paintColor = !brush.paintColor;
        if (c) ImGui::PopStyleColor();

        ImGui::SameLine();
        bool m = brush.paintMetallic;
        if (m) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button("Metal")) brush.paintMetallic = !brush.paintMetallic;
        if (m) ImGui::PopStyleColor();

        ImGui::SameLine();
        bool r = brush.paintRoughness;
        if (r) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button("Rough")) brush.paintRoughness = !brush.paintRoughness;
        if (r) ImGui::PopStyleColor();

        ImGui::SameLine();
        bool n = brush.paintNormal;
        if (n) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button("Nrm")) brush.paintNormal = !brush.paintNormal;
        if (n) ImGui::PopStyleColor();

        ImGui::SameLine();
        bool h = brush.paintHeight;
        if (h) ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0.14f, 0.44f, 0.82f, 1.0f));
        if (ImGui::Button("Height")) brush.paintHeight = !brush.paintHeight;
        if (h) ImGui::PopStyleColor();

        ImGui::Spacing();

        if (brush.paintColor) {
            float col[4] = { brush.color.r, brush.color.g, brush.color.b, brush.color.a };
            if (ImGui::ColorEdit4("Base Color", col)) {
                brush.color = glm::vec4(col[0], col[1], col[2], col[3]);
            }
        }

        if (brush.paintMetallic) {
            ImGui::SliderFloat("Metallic", &brush.metallic, 0.0f, 1.0f, "%.2f");
        }

        if (brush.paintRoughness) {
            ImGui::SliderFloat("Roughness", &brush.roughness, 0.0f, 1.0f, "%.2f");
        }

        if (brush.paintNormal) {
            ImGui::SliderFloat("Normal Z", &brush.normal.z, 0.1f, 1.0f, "%.2f");
        }

        if (brush.paintHeight) {
            ImGui::SliderFloat("Height Depth", &brush.height, -1.0f, 1.0f, "%.2f");
        }
    }

    ImGui::End();
}

void UIManager::renderStatusBar() {
    ImGuiViewport* viewport = ImGui::GetMainViewport();
    float height = 22.0f;
    ImGui::SetNextWindowPos(ImVec2(viewport->WorkPos.x, viewport->WorkPos.y + viewport->WorkSize.y - height));
    ImGui::SetNextWindowSize(ImVec2(viewport->WorkSize.x, height));

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoInputs |
                             ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoScrollWithMouse |
                             ImGuiWindowFlags_NoSavedSettings | ImGuiWindowFlags_NoBringToFrontOnFocus;

    ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.08f, 0.08f, 0.09f, 1.0f));
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(8, 2));
    ImGui::Begin("StatusBar##Window", nullptr, flags);

    ImGui::TextColored(ImVec4(0.6f, 0.8f, 1.0f, 1.0f), "Ready");
    ImGui::SameLine();
    ImGui::TextDisabled("| Active Tool: %s", (activeTool == ToolMode::Brush ? "Paint" : "Eraser"));
    ImGui::SameLine();
    ImGui::TextDisabled("| Res: %dx%d", textureResolution, textureResolution);

    ImGui::SameLine(ImGui::GetWindowWidth() - 420);
    ImGui::TextDisabled("Cache Disk Usage: 12%% | Version: 1.0.0 REMoAINTER | Vulkan 1.3");

    ImGui::End();
    ImGui::PopStyleVar();
    ImGui::PopStyleColor();
}

void UIManager::renderExportDialog(const LayerStack& layerStack) {
    if (!showExportModal) return;

    ImGui::OpenPopup("Export PBR Textures##Dialog");
    if (ImGui::BeginPopupModal("Export PBR Textures##Dialog", &showExportModal, ImGuiWindowFlags_AlwaysAutoResize)) {
        ImGui::Text("Export complete PBR texture set (Cook-Torrance standard maps):");
        ImGui::Separator();

        static char dirBuf[256] = "/mnt/shared";
        static char preBuf[64] = "REMoAINTER_Mat";

        ImGui::InputText("Export Directory", dirBuf, IM_ARRAYSIZE(dirBuf));
        ImGui::InputText("Texture Prefix", preBuf, IM_ARRAYSIZE(preBuf));

        ImGui::BulletText("%s_BaseColor.png", preBuf);
        ImGui::BulletText("%s_Normal.png (Tangent Space)", preBuf);
        ImGui::BulletText("%s_Roughness.png", preBuf);
        ImGui::BulletText("%s_Metallic.png", preBuf);
        ImGui::BulletText("%s_MetallicRoughness.png (glTF standard)", preBuf);

        ImGui::Separator();
        if (ImGui::Button("Export Textures Now", ImVec2(160, 0))) {
            bool ok = TextureExporter::exportPBRMaps(layerStack, dirBuf, preBuf);
            if (ok) statusMessage = "Exported textures to " + std::string(dirBuf);
            else statusMessage = "Failed to export textures";
            showExportModal = false;
        }
        ImGui::SameLine();
        if (ImGui::Button("Cancel", ImVec2(100, 0))) {
            showExportModal = false;
        }

        ImGui::EndPopup();
    }
}

// ==========================================
// Dynamic Preview Drawing Helpers
// ==========================================

void UIManager::drawStrokePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const Brush& brush) {
    // Dark background box
    drawList->AddRectFilled(minPos, maxPos, IM_COL32(20, 20, 23, 255), 2.0f);
    drawList->AddRect(minPos, maxPos, IM_COL32(40, 42, 46, 255), 2.0f);

    float midY = (minPos.y + maxPos.y) * 0.5f;
    float startX = minPos.x + 12.0f;
    float endX = maxPos.x - 12.0f;
    float width = endX - startX;

    // Draw smooth S-curved brush stroke preview
    float radius = std::clamp(brush.size * 0.22f, 2.0f, 14.0f);
    int colR = (int)(brush.color.r * 255);
    int colG = (int)(brush.color.g * 255);
    int colB = (int)(brush.color.b * 255);
    int alpha = (int)(brush.opacity * 255);

    const int steps = 40;
    for (int i = 0; i <= steps; ++i) {
        float t = (float)i / (float)steps;
        float x = startX + t * width;
        float y = midY + std::sin(t * 3.14159265f * 2.0f) * 18.0f;

        // Thickness varies along the stroke
        float r = radius * std::sin(t * 3.14159265f);
        drawList->AddCircleFilled(ImVec2(x, y), std::max(r, 1.5f), IM_COL32(colR, colG, colB, alpha));
    }
}

void UIManager::drawMaterialSpherePreview(ImDrawList* drawList, const ImVec2& minPos, const ImVec2& maxPos, const glm::vec4& color, float metallic, float roughness) {
    // Dark background box
    drawList->AddRectFilled(minPos, maxPos, IM_COL32(20, 20, 23, 255), 2.0f);
    drawList->AddRect(minPos, maxPos, IM_COL32(40, 42, 46, 255), 2.0f);

    ImVec2 center((minPos.x + maxPos.x) * 0.5f, (minPos.y + maxPos.y) * 0.5f);
    float radius = std::min(maxPos.x - minPos.x, maxPos.y - minPos.y) * 0.38f;

    drawShelfMaterialThumbnail(drawList, center, radius, color, metallic, roughness);
}

void UIManager::drawShelfMaterialThumbnail(ImDrawList* drawList, const ImVec2& center, float radius, const glm::vec4& color, float metallic, float roughness) {
    int colR = (int)(color.r * 255);
    int colG = (int)(color.g * 255);
    int colB = (int)(color.b * 255);

    // 1. Base Albedo Circle
    drawList->AddCircleFilled(center, radius, IM_COL32(colR, colG, colB, 255), 32);

    // 2. Ambient Shadow Crescent (Bottom-Left)
    ImVec2 shadowCenter(center.x + radius * 0.22f, center.y + radius * 0.22f);
    int shadowOpacity = (int)(110 + (1.0f - metallic) * 70);
    drawList->AddCircleFilled(shadowCenter, radius * 0.96f, IM_COL32(0, 0, 0, shadowOpacity), 32);

    // 3. Re-draw Albedo disc softly to blend shadow into curved sphere form
    drawList->AddCircleFilled(center, radius * 0.90f, IM_COL32(colR, colG, colB, 120), 32);

    // 4. Primary Specular Highlight (Top-Right key reflection)
    ImVec2 specPos(center.x - radius * 0.32f, center.y - radius * 0.32f);
    float specRadius = radius * (0.12f + roughness * 0.30f);
    int specAlpha = (int)(255 * (1.0f - roughness * 0.65f));

    // Metallic materials tint their specular highlight with base color
    int specR = (int)(255 * (1.0f - metallic) + colR * metallic);
    int specG = (int)(255 * (1.0f - metallic) + colG * metallic);
    int specB = (int)(255 * (1.0f - metallic) + colB * metallic);

    drawList->AddCircleFilled(specPos, specRadius, IM_COL32(specR, specG, specB, specAlpha), 20);

    // 5. Subtle Rim Light (Opposite edge)
    ImVec2 rimPos(center.x + radius * 0.45f, center.y + radius * 0.45f);
    drawList->AddCircleFilled(rimPos, radius * 0.20f, IM_COL32(255, 255, 255, 45), 16);

    // 6. Crisp Outer Edge Ring
    drawList->AddCircle(center, radius, IM_COL32(50, 54, 60, 255), 32, 1.2f);
}

void UIManager::renderAboutDialog() {
    if (showAboutModal) {
        ImGui::OpenPopup("About REMoAINTER##Modal");
        showAboutModal = false;
    }

    ImVec2 center = ImGui::GetMainViewport()->GetCenter();
    ImGui::SetNextWindowPos(center, ImGuiCond_Appearing, ImVec2(0.5f, 0.5f));
    ImGui::SetNextWindowSize(ImVec2(480, 420));

    if (ImGui::BeginPopupModal("About REMoAINTER##Modal", nullptr, ImGuiWindowFlags_NoResize)) {
        if (logoTextureID) {
            float imgSize = 140.0f;
            ImGui::SetCursorPosX((480 - imgSize) * 0.5f);
            ImGui::Image(logoTextureID, ImVec2(imgSize, imgSize));
            ImGui::Spacing();
        }

        const char* title = "REMoAINTER 1.1.0";
        float titleW = ImGui::CalcTextSize(title).x;
        ImGui::SetCursorPosX((480 - titleW) * 0.5f);
        ImGui::TextColored(ImVec4(0.96f, 0.55f, 0.15f, 1.0f), "%s", title);

        const char* sub = "Next-Gen 3D PBR Texture Painting Suite";
        float subW = ImGui::CalcTextSize(sub).x;
        ImGui::SetCursorPosX((480 - subW) * 0.5f);
        ImGui::TextDisabled("%s", sub);

        ImGui::Spacing();
        ImGui::Separator();
        ImGui::Spacing();

        ImGui::BulletText("Vulkan 1.3 High-Performance Viewport Engine");
        ImGui::BulletText("5-Light Studio PBR Rig with Dual Softbox Reflections & ACES Tonemapping");
        ImGui::BulletText("Substance 3D Painter Workflow & 40+ Pre-Configured PBR Smart Materials");
        ImGui::BulletText("Continuous Anti-Aliased Stroke Interpolation & Toroidal UV Seam Wrapping");
        ImGui::BulletText("Zero-Stutter Persistent GPU Staging & Dirty-Rect Streaming");
        ImGui::BulletText("Linux & Windows High-Performance Native Support");

        ImGui::Spacing();
        ImGui::Separator();
        ImGui::Spacing();

        ImGui::SetCursorPosX((480 - 110) * 0.5f);
        if (ImGui::Button("Close", ImVec2(110, 30))) {
            ImGui::CloseCurrentPopup();
        }

        ImGui::EndPopup();
    }
}

} // namespace REMoAINTER
