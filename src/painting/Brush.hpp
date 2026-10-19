#pragma once

#include <glm/glm.hpp>
#include <string>

namespace REMoAINTER {

struct Brush {
    float size = 32.0f;              // Radius in texture pixels (1 to 256)
    float minSizePercent = 1.0f;     // Minimum size percentage for pressure (1 to 100)
    float hardness = 0.85f;          // Edge softness (0.0 = ultra soft, 1.0 = razor sharp)
    float opacity = 1.0f;            // Maximum stroke opacity ceiling (0.0 to 1.0)
    float flow = 1.0f;               // Paint flow accumulation per stamp (0.0 to 1.0)
    float spacing = 0.12f;           // Distance ratio between consecutive stamps (12% of brush size)
    float angle = 0.0f;              // Base brush rotation angle in radians
    bool followPath = false;         // Automatically rotate brush angle along stroke direction
    bool lazyMouse = true;           // Smooth stroke / lazy mouse damping
    float lazyRadius = 0.35f;        // Lazy mouse smoothing factor (0.0 to 0.9)
    bool penPressure = true;         // Pressure sensitivity simulation

    int alphaType = 0;               // 0: Round Hard, 1: Round Soft, 2: Noise, 3: Scratches, 4: Stipple, 5: Square, 6: Star
    std::string alphaName = "Round Hard";

    // PBR Material values painted by this brush
    bool paintColor = true;
    glm::vec4 color{0.85f, 0.25f, 0.20f, 1.0f}; // Base Color (sRGB)

    bool paintMetallic = true;
    float metallic = 0.0f;           // 0.0 = Dielectric, 1.0 = Metal

    bool paintRoughness = true;
    float roughness = 0.35f;         // 0.0 = Glossy mirror, 1.0 = Rough matte

    bool paintNormal = true;
    glm::vec3 normal{0.0f, 0.0f, 1.0f}; // Tangent space normal (0, 0, 1 = flat)

    bool paintHeight = false;
    float height = 0.0f;             // Height relief (-1.0 to +1.0)

    bool isEraser = false;           // Eraser mode
};

} // namespace REMoAINTER
