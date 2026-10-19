#pragma once

#include "../../third_party/imgui/imgui.h"

namespace REMoAINTER {

inline void applyDarkSubstanceTheme() {
    ImGuiStyle& style = ImGui::GetStyle();
    ImVec4* colors = style.Colors;

    // Window & Backgrounds
    colors[ImGuiCol_WindowBg]             = ImVec4(0.13f, 0.14f, 0.16f, 1.00f);
    colors[ImGuiCol_ChildBg]              = ImVec4(0.15f, 0.16f, 0.18f, 1.00f);
    colors[ImGuiCol_PopupBg]              = ImVec4(0.18f, 0.19f, 0.22f, 0.98f);
    colors[ImGuiCol_Border]               = ImVec4(0.24f, 0.26f, 0.30f, 0.70f);
    colors[ImGuiCol_BorderShadow]         = ImVec4(0.00f, 0.00f, 0.00f, 0.00f);

    // Headers & Titles
    colors[ImGuiCol_TitleBg]              = ImVec4(0.11f, 0.12f, 0.13f, 1.00f);
    colors[ImGuiCol_TitleBgActive]        = ImVec4(0.16f, 0.18f, 0.22f, 1.00f);
    colors[ImGuiCol_TitleBgCollapsed]     = ImVec4(0.10f, 0.11f, 0.12f, 0.80f);
    colors[ImGuiCol_MenuBarBg]            = ImVec4(0.11f, 0.12f, 0.14f, 1.00f);

    // Text & Controls
    colors[ImGuiCol_Text]                 = ImVec4(0.92f, 0.93f, 0.95f, 1.00f);
    colors[ImGuiCol_TextDisabled]         = ImVec4(0.50f, 0.52f, 0.56f, 1.00f);

    // Buttons (Substance orange-cyan accents)
    colors[ImGuiCol_Button]               = ImVec4(0.22f, 0.25f, 0.30f, 1.00f);
    colors[ImGuiCol_ButtonHovered]        = ImVec4(0.30f, 0.35f, 0.42f, 1.00f);
    colors[ImGuiCol_ButtonActive]         = ImVec4(0.18f, 0.45f, 0.65f, 1.00f);

    // Frame (Inputs, sliders, checkboxes)
    colors[ImGuiCol_FrameBg]              = ImVec4(0.18f, 0.20f, 0.23f, 1.00f);
    colors[ImGuiCol_FrameBgHovered]       = ImVec4(0.24f, 0.27f, 0.32f, 1.00f);
    colors[ImGuiCol_FrameBgActive]        = ImVec4(0.28f, 0.32f, 0.38f, 1.00f);

    // Tabs
    colors[ImGuiCol_Tab]                  = ImVec4(0.15f, 0.17f, 0.20f, 1.00f);
    colors[ImGuiCol_TabHovered]           = ImVec4(0.26f, 0.30f, 0.38f, 1.00f);
    colors[ImGuiCol_TabSelected]          = ImVec4(0.20f, 0.23f, 0.28f, 1.00f);
    colors[ImGuiCol_TabSelectedOverline]  = ImVec4(0.85f, 0.45f, 0.15f, 1.00f); // Accent orange
    colors[ImGuiCol_TabDimmed]            = ImVec4(0.12f, 0.13f, 0.15f, 1.00f);
    colors[ImGuiCol_TabDimmedSelected]    = ImVec4(0.16f, 0.18f, 0.22f, 1.00f);

    // Sliders & Scrollbars
    colors[ImGuiCol_SliderGrab]           = ImVec4(0.85f, 0.45f, 0.15f, 1.00f); // Accent orange
    colors[ImGuiCol_SliderGrabActive]     = ImVec4(0.95f, 0.55f, 0.20f, 1.00f);
    colors[ImGuiCol_CheckMark]            = ImVec4(0.85f, 0.45f, 0.15f, 1.00f);

    colors[ImGuiCol_ScrollbarBg]          = ImVec4(0.11f, 0.12f, 0.13f, 0.60f);
    colors[ImGuiCol_ScrollbarGrab]        = ImVec4(0.24f, 0.26f, 0.30f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabHovered] = ImVec4(0.32f, 0.35f, 0.40f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabActive]  = ImVec4(0.40f, 0.44f, 0.50f, 1.00f);

    // Docking
    colors[ImGuiCol_DockingPreview]       = ImVec4(0.85f, 0.45f, 0.15f, 0.40f);
    colors[ImGuiCol_DockingEmptyBg]       = ImVec4(0.10f, 0.10f, 0.11f, 1.00f);

    // Style tweaks
    style.WindowRounding    = 4.0f;
    style.ChildRounding     = 3.0f;
    style.FrameRounding     = 3.0f;
    style.PopupRounding     = 4.0f;
    style.ScrollbarRounding = 4.0f;
    style.GrabRounding      = 3.0f;
    style.TabRounding       = 4.0f;

    style.WindowPadding     = ImVec2(8.0f, 8.0f);
    style.FramePadding      = ImVec2(6.0f, 4.0f);
    style.ItemSpacing       = ImVec2(8.0f, 6.0f);
    style.ItemInnerSpacing  = ImVec2(6.0f, 4.0f);
}

} // namespace REMoAINTER
