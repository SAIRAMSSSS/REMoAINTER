#pragma once

#include "Brush.hpp"
#include <string>
#include <vector>
#include <cstdint>
#include <glm/glm.hpp>

namespace REMoAINTER {

enum class LayerType {
    Paint,
    Fill
};

enum class BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    LinearAdd
};

class Layer {
public:
    std::string name = "New Layer";
    LayerType type = LayerType::Paint;
    bool visible = true;
    float opacity = 1.0f;
    BlendMode blendMode = BlendMode::Normal;

    uint32_t width = 1024;
    uint32_t height = 1024;

    // Channels enabled on this layer
    bool channelColor = true;
    bool channelMetallic = true;
    bool channelRoughness = true;
    bool channelNormal = true;

    // Fill Layer uniform properties
    glm::vec4 fillColor{0.7f, 0.7f, 0.7f, 1.0f};
    float fillMetallic = 0.0f;
    float fillRoughness = 0.5f;
    glm::vec3 fillNormal{0.0f, 0.0f, 1.0f};

    // Paint Layer texture channel buffers (RGBA8)
    std::vector<uint8_t> colorData;   // RGBA8
    std::vector<uint8_t> mrData;      // R: AO (255), G: Roughness, B: Metallic, A: Mask/Coverage
    std::vector<uint8_t> normalData;  // R: Nx, G: Ny, B: Nz, A: Height

    Layer(const std::string& name, LayerType type, uint32_t w = 1024, uint32_t h = 1024);

    void resize(uint32_t w, uint32_t h);
    void clear();

    void paintStamp(float uvX, float uvY, const Brush& brush,
                    int* outMinX = nullptr, int* outMinY = nullptr, int* outMaxX = nullptr, int* outMaxY = nullptr);
    void paintLine(const glm::vec2& p0, const glm::vec2& p1, const Brush& brush,
                   int* outMinX = nullptr, int* outMinY = nullptr, int* outMaxX = nullptr, int* outMaxY = nullptr);
};

} // namespace REMoAINTER
