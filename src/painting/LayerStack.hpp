#pragma once

#include "Layer.hpp"
#include <memory>
#include <vector>

namespace REMoAINTER {

class LayerStack {
public:
    uint32_t width = 1024;
    uint32_t height = 1024;

    std::vector<std::shared_ptr<Layer>> layers;
    int activeLayerIndex = 0;

    // Final composited texture buffers
    std::vector<uint8_t> compositedBaseColor;
    std::vector<uint8_t> compositedMetallicRoughness;
    std::vector<uint8_t> compositedNormal;

    bool isDirty = true;

    LayerStack(uint32_t w = 1024, uint32_t h = 1024);

    void addPaintLayer(const std::string& name = "Paint Layer");
    void addFillLayer(const std::string& name = "Fill Layer", const glm::vec4& color = glm::vec4(0.7f, 0.7f, 0.7f, 1.0f));
    void removeLayer(int index);
    void moveLayerUp(int index);
    void moveLayerDown(int index);

    std::shared_ptr<Layer> getActiveLayer();

    void composite();
};

} // namespace REMoAINTER
