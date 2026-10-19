#pragma once

#include "../../third_party/imgui/imgui.h"

namespace REMoAINTER {

inline void applyDarkSubstanceTheme() {
    ImGuiStyle& style = ImGui::GetStyle();
    ImVec4* colors = style.Colors;

    // Substance 3D Painter Authentic Dark Charcoal & Cyan-Blue Accent Palette
    const ImVec4 bgVeryDark      = ImVec4(0.09f, 0.09f, 0.10f, 1.00f); // #17171a
    const ImVec4 bgWindow        = ImVec4(0.11f, 0.11f, 0.12f, 1.00f); // #1c1c1f
    const ImVec4 bgPanel         = ImVec4(0.13f, 0.13f, 0.14f, 1.00f); // #212124
    const ImVec4 bgFrame         = ImVec4(0.16f, 0.16f, 0.18f, 1.00f); // #29292e
    const ImVec4 bgFrameHover    = ImVec4(0.20f, 0.21f, 0.24f, 1.00f); // #33363d
    const ImVec4 bgFrameActive   = ImVec4(0.24f, 0.25f, 0.29f, 1.00f);

    const ImVec4 adobeBlue       = ImVec4(0.14f, 0.44f, 0.82f, 1.00f); // #2470d1 (Substance Selection Blue)
    const ImVec4 adobeBlueHover  = ImVec4(0.20f, 0.52f, 0.92f, 1.00f); // #3385eb
    const ImVec4 adobeBlueActive = ImVec4(0.10f, 0.38f, 0.72f, 1.00f);
    const ImVec4 accentOrange    = ImVec4(0.95f, 0.50f, 0.15f, 1.00f); // Brand orange

    const ImVec4 textWhite       = ImVec4(0.88f, 0.89f, 0.92f, 1.00f);
    const ImVec4 textDim         = ImVec4(0.52f, 0.54f, 0.58f, 1.00f);
    const ImVec4 borderDark      = ImVec4(0.18f, 0.19f, 0.21f, 0.85f);

    // Windows & Panels
    colors[ImGuiCol_WindowBg]             = bgWindow;
    colors[ImGuiCol_ChildBg]              = bgPanel;
    colors[ImGuiCol_PopupBg]              = ImVec4(0.12f, 0.12f, 0.14f, 0.98f);
    colors[ImGuiCol_Border]               = borderDark;
    colors[ImGuiCol_BorderShadow]         = ImVec4(0.00f, 0.00f, 0.00f, 0.00f);

    // Menus & Titles
    colors[ImGuiCol_MenuBarBg]            = bgVeryDark;
    colors[ImGuiCol_TitleBg]              = bgVeryDark;
    colors[ImGuiCol_TitleBgActive]        = bgVeryDark;
    colors[ImGuiCol_TitleBgCollapsed]     = bgVeryDark;

    // Text
    colors[ImGuiCol_Text]                 = textWhite;
    colors[ImGuiCol_TextDisabled]         = textDim;

    // Controls & Buttons
    colors[ImGuiCol_Button]               = bgFrame;
    colors[ImGuiCol_ButtonHovered]        = bgFrameHover;
    colors[ImGuiCol_ButtonActive]         = adobeBlue;

    colors[ImGuiCol_FrameBg]              = bgFrame;
    colors[ImGuiCol_FrameBgHovered]       = bgFrameHover;
    colors[ImGuiCol_FrameBgActive]        = bgFrameActive;

    // Headers & Selections (Matching Substance Painter blue active state)
    colors[ImGuiCol_Header]               = ImVec4(0.14f, 0.44f, 0.82f, 0.55f);
    colors[ImGuiCol_HeaderHovered]        = adobeBlueHover;
    colors[ImGuiCol_HeaderActive]         = adobeBlue;

    // Tabs
    colors[ImGuiCol_Tab]                  = bgVeryDark;
    colors[ImGuiCol_TabHovered]           = bgFrameHover;
    colors[ImGuiCol_TabSelected]          = bgPanel;
    colors[ImGuiCol_TabSelectedOverline]  = adobeBlue;
    colors[ImGuiCol_TabDimmed]            = bgVeryDark;
    colors[ImGuiCol_TabDimmedSelected]    = bgPanel;

    // Sliders & Checkmarks
    colors[ImGuiCol_SliderGrab]           = adobeBlue;
    colors[ImGuiCol_SliderGrabActive]     = adobeBlueHover;
    colors[ImGuiCol_CheckMark]            = adobeBlue;

    // Scrollbars
    colors[ImGuiCol_ScrollbarBg]          = ImVec4(0.08f, 0.08f, 0.09f, 0.50f);
    colors[ImGuiCol_ScrollbarGrab]        = ImVec4(0.22f, 0.23f, 0.25f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabHovered] = ImVec4(0.30f, 0.32f, 0.36f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabActive]  = adobeBlue;

    // Docking
    colors[ImGuiCol_DockingPreview]       = ImVec4(0.14f, 0.44f, 0.82f, 0.60f);
    colors[ImGuiCol_DockingEmptyBg]       = bgVeryDark;

    // Style Geometry Tweaks
    style.WindowRounding    = 2.0f;
    style.ChildRounding     = 2.0f;
    style.FrameRounding     = 2.0f;
    style.PopupRounding     = 2.0f;
    style.ScrollbarRounding = 2.0f;
    style.GrabRounding      = 2.0f;
    style.TabRounding       = 2.0f;

    style.WindowPadding     = ImVec2(6.0f, 6.0f);
    style.FramePadding      = ImVec2(5.0f, 3.5f);
    style.ItemSpacing       = ImVec2(6.0f, 5.0f);
    style.ItemInnerSpacing  = ImVec2(4.0f, 4.0f);
    style.WindowBorderSize  = 1.0f;
    style.FrameBorderSize   = 0.0f;
    style.PopupBorderSize   = 1.0f;
}

} // namespace REMoAINTER
