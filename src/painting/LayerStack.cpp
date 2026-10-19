#include "LayerStack.hpp"
#include <algorithm>
#include <cmath>

namespace REMoAINTER {

LayerStack::LayerStack(uint32_t w, uint32_t h) : width(w), height(h) {
    size_t numPixels = (size_t)width * height;
    compositedBaseColor.resize(numPixels * 4);
    compositedMetallicRoughness.resize(numPixels * 4);
    compositedNormal.resize(numPixels * 4);

    // Default: Add a base fill layer and an active paint layer
    addFillLayer("Base Material", glm::vec4(0.8f, 0.8f, 0.82f, 1.0f));
    addPaintLayer("Paint Layer 1");
    activeLayerIndex = 1;

    composite();
}

void LayerStack::addPaintLayer(const std::string& name) {
    auto layer = std::make_shared<Layer>(name, LayerType::Paint, width, height);
    layers.push_back(layer);
    activeLayerIndex = (int)layers.size() - 1;
    isDirty = true;
}

void LayerStack::addFillLayer(const std::string& name, const glm::vec4& color) {
    auto layer = std::make_shared<Layer>(name, LayerType::Fill, width, height);
    layer->fillColor = color;
    layer->fillRoughness = 0.45f;
    layer->fillMetallic = 0.0f;
    layer->clear();
    layers.push_back(layer);
    activeLayerIndex = (int)layers.size() - 1;
    isDirty = true;
}

void LayerStack::removeLayer(int index) {
    if (index >= 0 && index < (int)layers.size() && layers.size() > 1) {
        layers.erase(layers.begin() + index);
        activeLayerIndex = std::clamp(activeLayerIndex, 0, (int)layers.size() - 1);
        isDirty = true;
    }
}

void LayerStack::moveLayerUp(int index) {
    if (index < (int)layers.size() - 1) {
        std::swap(layers[index], layers[index + 1]);
        if (activeLayerIndex == index) activeLayerIndex++;
        else if (activeLayerIndex == index + 1) activeLayerIndex--;
        isDirty = true;
    }
}

void LayerStack::moveLayerDown(int index) {
    if (index > 0) {
        std::swap(layers[index], layers[index - 1]);
        if (activeLayerIndex == index) activeLayerIndex--;
        else if (activeLayerIndex == index - 1) activeLayerIndex++;
        isDirty = true;
    }
}

std::shared_ptr<Layer> LayerStack::getActiveLayer() {
    if (activeLayerIndex >= 0 && activeLayerIndex < (int)layers.size()) {
        return layers[activeLayerIndex];
    }
    return nullptr;
}

static inline float applyBlendMode(float bg, float fg, BlendMode mode) {
    switch (mode) {
        case BlendMode::Multiply:
            return bg * fg;
        case BlendMode::Screen:
            return 1.0f - (1.0f - bg) * (1.0f - fg);
        case BlendMode::Overlay:
            return (bg < 0.5f) ? (2.0f * bg * fg) : (1.0f - 2.0f * (1.0f - bg) * (1.0f - fg));
        case BlendMode::LinearAdd:
            return std::min(1.0f, bg + fg);
        case BlendMode::Normal:
        default:
            return fg;
    }
}

void LayerStack::composite() {
    size_t numPixels = (size_t)width * height;

    // Default base environment color: neutral grey, medium roughness, non-metallic, flat normal
    for (size_t i = 0; i < numPixels; ++i) {
        compositedBaseColor[i * 4 + 0] = 180;
        compositedBaseColor[i * 4 + 1] = 180;
        compositedBaseColor[i * 4 + 2] = 180;
        compositedBaseColor[i * 4 + 3] = 255;

        compositedMetallicRoughness[i * 4 + 0] = 255; // AO
        compositedMetallicRoughness[i * 4 + 1] = 128; // Roughness 0.5
        compositedMetallicRoughness[i * 4 + 2] = 0;   // Metallic 0.0
        compositedMetallicRoughness[i * 4 + 3] = 255;

        compositedNormal[i * 4 + 0] = 128; // Nx = 0
        compositedNormal[i * 4 + 1] = 128; // Ny = 0
        compositedNormal[i * 4 + 2] = 255; // Nz = 1
        compositedNormal[i * 4 + 3] = 255;
    }

    // Blend layers from bottom (index 0) to top
    for (const auto& layer : layers) {
        if (!layer->visible || layer->opacity <= 0.001f) continue;

        float layerOpacity = layer->opacity;

        for (size_t i = 0; i < numPixels; ++i) {
            size_t idx = i * 4;

            float srcA = (layer->colorData[idx + 3] / 255.0f) * layerOpacity;
            if (srcA <= 0.001f && layer->type == LayerType::Paint) continue;

            if (layer->type == LayerType::Fill) {
                srcA = layerOpacity;
            }

            // 1. Composite Base Color
            if (layer->channelColor) {
                float bgR = compositedBaseColor[idx + 0] / 255.0f;
                float bgG = compositedBaseColor[idx + 1] / 255.0f;
                float bgB = compositedBaseColor[idx + 2] / 255.0f;

                float fgR = layer->colorData[idx + 0] / 255.0f;
                float fgG = layer->colorData[idx + 1] / 255.0f;
                float fgB = layer->colorData[idx + 2] / 255.0f;

                float blendedR = applyBlendMode(bgR, fgR, layer->blendMode);
                float blendedG = applyBlendMode(bgG, fgG, layer->blendMode);
                float blendedB = applyBlendMode(bgB, fgB, layer->blendMode);

                compositedBaseColor[idx + 0] = (uint8_t)std::clamp((bgR * (1.0f - srcA) + blendedR * srcA) * 255.0f, 0.0f, 255.0f);
                compositedBaseColor[idx + 1] = (uint8_t)std::clamp((bgG * (1.0f - srcA) + blendedG * srcA) * 255.0f, 0.0f, 255.0f);
                compositedBaseColor[idx + 2] = (uint8_t)std::clamp((bgB * (1.0f - srcA) + blendedB * srcA) * 255.0f, 0.0f, 255.0f);
            }

            // 2. Composite Roughness & Metallic
            float mrCoverage = (layer->mrData[idx + 3] / 255.0f) * layerOpacity;
            if (layer->type == LayerType::Fill) mrCoverage = layerOpacity;

            if (mrCoverage > 0.001f) {
                if (layer->channelRoughness) {
                    float bgRough = compositedMetallicRoughness[idx + 1];
                    float fgRough = layer->mrData[idx + 1];
                    compositedMetallicRoughness[idx + 1] = (uint8_t)std::clamp(bgRough * (1.0f - mrCoverage) + fgRough * mrCoverage, 0.0f, 255.0f);
                }
                if (layer->channelMetallic) {
                    float bgMetal = compositedMetallicRoughness[idx + 2];
                    float fgMetal = layer->mrData[idx + 2];
                    compositedMetallicRoughness[idx + 2] = (uint8_t)std::clamp(bgMetal * (1.0f - mrCoverage) + fgMetal * mrCoverage, 0.0f, 255.0f);
                }
            }

            // 3. Composite Tangent Space Normals
            if (layer->channelNormal && srcA > 0.01f) {
                float n1x = (compositedNormal[idx + 0] / 255.0f) * 2.0f - 1.0f;
                float n1y = (compositedNormal[idx + 1] / 255.0f) * 2.0f - 1.0f;
                float n1z = (compositedNormal[idx + 2] / 255.0f) * 2.0f - 1.0f;

                float n2x = ((layer->normalData[idx + 0] / 255.0f) * 2.0f - 1.0f) * srcA;
                float n2y = ((layer->normalData[idx + 1] / 255.0f) * 2.0f - 1.0f) * srcA;
                float n2z = (layer->normalData[idx + 2] / 255.0f) * 2.0f - 1.0f;

                // Reoriented normal blend: n1 + vec3(n2.xy, 0)
                float outX = n1x + n2x;
                float outY = n1y + n2y;
                float outZ = n1z;
                float len = std::sqrt(outX * outX + outY * outY + outZ * outZ);
                if (len > 0.0001f) {
                    outX /= len;
                    outY /= len;
                    outZ /= len;
                }

                compositedNormal[idx + 0] = (uint8_t)std::clamp((outX * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
                compositedNormal[idx + 1] = (uint8_t)std::clamp((outY * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
                compositedNormal[idx + 2] = (uint8_t)std::clamp((outZ * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
            }
        }
    }

    isDirty = false;
}

} // namespace REMoAINTER
