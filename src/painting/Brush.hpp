#pragma once

#include <glm/glm.hpp>

namespace REMoAINTER {

struct Brush {
    float size = 32.0f;       // Radius in texture pixels (1 to 256)
    float hardness = 0.8f;    // Edge softness (0.0 = fuzzy gaussian, 1.0 = sharp)
    float opacity = 1.0f;     // Stroke strength (0.0 to 1.0)
    float flow = 1.0f;        // Continuous flow rate
    float spacing = 0.15f;    // Distance ratio between stamps
    float angle = 0.0f;       // Brush rotation angle

    int alphaType = 0;        // 0: RoundHard, 1: RoundSoft, 2: Noise, 3: Scratches, 4: Stipple, 5: Square, 6: Star

    // PBR Material values painted by this brush
    bool paintColor = true;
    glm::vec4 color{0.85f, 0.25f, 0.2f, 1.0f}; // Base Color (sRGB)

    bool paintMetallic = true;
    float metallic = 0.0f;    // 0.0 = Dielectric, 1.0 = Metal

    bool paintRoughness = true;
    float roughness = 0.35f;  // 0.0 = Glossy mirror, 1.0 = Rough matte

    bool paintNormal = false;
    glm::vec3 normal{0.0f, 0.0f, 1.0f}; // Tangent space normal deviation

    bool isEraser = false;
};

} // namespace REMoAINTER
