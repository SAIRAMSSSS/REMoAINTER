#pragma once

#include "../../third_party/imgui/imgui.h"

namespace REMoAINTER {

inline void applyDarkSubstanceTheme() {
    ImGuiStyle& style = ImGui::GetStyle();
    ImVec4* colors = style.Colors;

    // Allegorithmic / Adobe Substance 3D Painter Official Palette
    // Exact colors extracted from AlgStyle.qml & AlgWidgetsStyle.qml:
    // blue: #1A8DFF (rgb 26, 141, 255)
    // gray(20): #333333 (rgb 51, 51, 51)
    // gray(15): #262626 (rgb 38, 38, 38)
    // gray(10): #1A1A1A (rgb 26, 26, 26)
    // gray(25): #404040 (rgb 64, 64, 64)
    // gray(30): #4D4D4D (rgb 77, 77, 77)
    // gray(40): #666666 (rgb 102, 102, 102)
    // gray(80): #CCCCCC (rgb 204, 204, 204)
    // alizarin: rgb(239, 78, 53)
    // carott: rgb(239, 126, 34)

    const ImVec4 subBlue         = ImVec4(0.102f, 0.553f, 1.000f, 1.00f); // #1A8DFF
    const ImVec4 subBlueHover    = ImVec4(0.278f, 0.639f, 1.000f, 1.00f); // #47A3FF
    const ImVec4 subBlueActive   = ImVec4(0.043f, 0.459f, 0.882f, 1.00f); // #0B75E1
    const ImVec4 subBlueDim      = ImVec4(0.102f, 0.553f, 1.000f, 0.35f);

    const ImVec4 bgWindow        = ImVec4(0.150f, 0.150f, 0.150f, 1.00f); // gray(15)-gray(20) ~ #262626
    const ImVec4 bgPanel         = ImVec4(0.130f, 0.130f, 0.130f, 1.00f); // #212121
    const ImVec4 bgFrame         = ImVec4(0.090f, 0.090f, 0.090f, 1.00f); // gray(10)-gray(15) ~ #171717
    const ImVec4 bgFrameHover    = ImVec4(0.180f, 0.180f, 0.180f, 1.00f); // #2E2E2E
    const ImVec4 bgFrameActive   = ImVec4(0.220f, 0.220f, 0.220f, 1.00f); // #383838

    const ImVec4 textNormal      = ImVec4(0.800f, 0.800f, 0.800f, 1.00f); // gray(80) #CCCCCC
    const ImVec4 textDimmed      = ImVec4(0.480f, 0.480f, 0.480f, 1.00f); // gray(40)-gray(50)
    const ImVec4 textWhite       = ImVec4(0.960f, 0.960f, 0.960f, 1.00f);

    const ImVec4 borderDark      = ImVec4(0.080f, 0.080f, 0.080f, 1.00f); // #141414
    const ImVec4 borderLight     = ImVec4(0.220f, 0.220f, 0.220f, 0.80f);

    // Windows & Child Panels
    colors[ImGuiCol_WindowBg]             = bgWindow;
    colors[ImGuiCol_ChildBg]              = bgPanel;
    colors[ImGuiCol_PopupBg]              = ImVec4(0.110f, 0.110f, 0.110f, 0.98f);
    colors[ImGuiCol_Border]               = borderDark;
    colors[ImGuiCol_BorderShadow]         = ImVec4(0.00f, 0.00f, 0.00f, 0.00f);

    // Menus & Titles
    colors[ImGuiCol_MenuBarBg]            = ImVec4(0.100f, 0.100f, 0.100f, 1.00f); // #1A1A1A
    colors[ImGuiCol_TitleBg]              = ImVec4(0.120f, 0.120f, 0.120f, 1.00f);
    colors[ImGuiCol_TitleBgActive]        = ImVec4(0.120f, 0.120f, 0.120f, 1.00f);
    colors[ImGuiCol_TitleBgCollapsed]     = ImVec4(0.100f, 0.100f, 0.100f, 1.00f);

    // Text
    colors[ImGuiCol_Text]                 = textNormal;
    colors[ImGuiCol_TextDisabled]         = textDimmed;

    // Buttons
    colors[ImGuiCol_Button]               = ImVec4(0.160f, 0.160f, 0.160f, 1.00f);
    colors[ImGuiCol_ButtonHovered]        = ImVec4(0.220f, 0.220f, 0.220f, 1.00f);
    colors[ImGuiCol_ButtonActive]         = subBlue;

    // Inputs / Frames
    colors[ImGuiCol_FrameBg]              = bgFrame;
    colors[ImGuiCol_FrameBgHovered]       = bgFrameHover;
    colors[ImGuiCol_FrameBgActive]        = bgFrameActive;

    // Headers & Selections (Substance Painter #1A8DFF selection bar)
    colors[ImGuiCol_Header]               = subBlueDim;
    colors[ImGuiCol_HeaderHovered]        = ImVec4(0.102f, 0.553f, 1.000f, 0.65f);
    colors[ImGuiCol_HeaderActive]         = subBlue;

    // Tabs
    colors[ImGuiCol_Tab]                  = ImVec4(0.110f, 0.110f, 0.110f, 1.00f);
    colors[ImGuiCol_TabHovered]           = ImVec4(0.180f, 0.180f, 0.180f, 1.00f);
    colors[ImGuiCol_TabSelected]          = ImVec4(0.150f, 0.150f, 0.150f, 1.00f);
    colors[ImGuiCol_TabSelectedOverline]  = subBlue;
    colors[ImGuiCol_TabDimmed]            = ImVec4(0.100f, 0.100f, 0.100f, 1.00f);
    colors[ImGuiCol_TabDimmedSelected]    = ImVec4(0.140f, 0.140f, 0.140f, 1.00f);

    // Sliders & Checkmarks
    colors[ImGuiCol_SliderGrab]           = subBlue;
    colors[ImGuiCol_SliderGrabActive]     = subBlueHover;
    colors[ImGuiCol_CheckMark]            = subBlue;

    // Scrollbars
    colors[ImGuiCol_ScrollbarBg]          = ImVec4(0.080f, 0.080f, 0.080f, 0.50f);
    colors[ImGuiCol_ScrollbarGrab]        = ImVec4(0.250f, 0.250f, 0.250f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabHovered] = ImVec4(0.350f, 0.350f, 0.350f, 1.00f);
    colors[ImGuiCol_ScrollbarGrabActive]  = subBlue;

    // Docking
    colors[ImGuiCol_DockingPreview]       = ImVec4(0.102f, 0.553f, 1.000f, 0.60f);
    colors[ImGuiCol_DockingEmptyBg]       = ImVec4(0.080f, 0.080f, 0.080f, 1.00f);

    // Substance-Accurate Metrics
    style.WindowRounding    = 0.0f; // Substance Painter panels have sharp docked edges
    style.ChildRounding     = 0.0f;
    style.FrameRounding     = 3.0f; // Sliders and buttons have subtle 3px rounding
    style.PopupRounding     = 3.0f;
    style.ScrollbarRounding = 2.0f;
    style.GrabRounding      = 2.0f;
    style.TabRounding       = 0.0f; // Tabs are flush and square

    style.WindowPadding     = ImVec2(6.0f, 6.0f);
    style.FramePadding      = ImVec2(6.0f, 3.5f);
    style.ItemSpacing       = ImVec2(6.0f, 4.0f);
    style.ItemInnerSpacing  = ImVec2(4.0f, 4.0f);
    style.WindowBorderSize  = 1.0f;
    style.FrameBorderSize   = 1.0f;
    style.PopupBorderSize   = 1.0f;
    style.ScrollbarSize     = 10.0f;
}

} // namespace REMoAINTER
