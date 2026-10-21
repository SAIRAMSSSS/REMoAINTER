#include "LayerStack.hpp"
#include <algorithm>
#include <cmath>

namespace REMoAINTER {

LayerStack::LayerStack(uint32_t w, uint32_t h) : width(w), height(h) {
    size_t numPixels = (size_t)width * height;
    compositedBaseColor.resize(numPixels * 4);
    compositedMetallicRoughness.resize(numPixels * 4);
    compositedNormal.resize(numPixels * 4);

    // Default: Realistic Substance Painter hierarchy matching reference
    addFillLayer("Base", glm::vec4(0.82f, 0.82f, 0.84f, 1.0f));
    addFolder("Scattering");
    addFolder("Color");
    addFolder("Roughness");
    addFolder("Final Touches");
    layers.back()->blendMode = BlendMode::PassThrough;
    addPaintLayer("Layer 1");
    activeLayerIndex = 5; // "Layer 1" active on top

    composite();
}

void LayerStack::addPaintLayer(const std::string& name) {
    auto layer = std::make_shared<Layer>(name, LayerType::Paint, width, height);
    layers.push_back(layer);
    activeLayerIndex = (int)layers.size() - 1;
    markFullDirty();
}

void LayerStack::addFillLayer(const std::string& name, const glm::vec4& color) {
    auto layer = std::make_shared<Layer>(name, LayerType::Fill, width, height);
    layer->fillColor = color;
    layer->fillRoughness = 0.45f;
    layer->fillMetallic = 0.0f;
    layer->clear();
    layers.push_back(layer);
    activeLayerIndex = (int)layers.size() - 1;
    markFullDirty();
}

void LayerStack::addFolder(const std::string& name) {
    auto layer = std::make_shared<Layer>(name, LayerType::Folder, width, height);
    layers.push_back(layer);
    activeLayerIndex = (int)layers.size() - 1;
    markFullDirty();
}

void LayerStack::removeLayer(int index) {
    if (index >= 0 && index < (int)layers.size() && layers.size() > 1) {
        layers.erase(layers.begin() + index);
        activeLayerIndex = std::clamp(activeLayerIndex, 0, (int)layers.size() - 1);
        markFullDirty();
    }
}

void LayerStack::moveLayerUp(int index) {
    if (index < (int)layers.size() - 1) {
        std::swap(layers[index], layers[index + 1]);
        if (activeLayerIndex == index) activeLayerIndex++;
        else if (activeLayerIndex == index + 1) activeLayerIndex--;
        markFullDirty();
    }
}

void LayerStack::moveLayerDown(int index) {
    if (index > 0) {
        std::swap(layers[index], layers[index - 1]);
        if (activeLayerIndex == index) activeLayerIndex--;
        else if (activeLayerIndex == index - 1) activeLayerIndex++;
        markFullDirty();
    }
}

std::shared_ptr<Layer> LayerStack::getActiveLayer() {
    if (activeLayerIndex >= 0 && activeLayerIndex < (int)layers.size()) {
        return layers[activeLayerIndex];
    }
    return nullptr;
}

void LayerStack::resize(uint32_t w, uint32_t h) {
    width = w;
    height = h;
    size_t numPixels = (size_t)width * height;
    compositedBaseColor.resize(numPixels * 4);
    compositedMetallicRoughness.resize(numPixels * 4);
    compositedNormal.resize(numPixels * 4);

    for (auto& layer : layers) {
        layer->resize(w, h);
    }
    composite();
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

void LayerStack::markDirtyRect(int minX, int minY, int maxX, int maxY) {
    if (!isDirty || fullDirty) {
        if (!fullDirty) {
            dirtyMinX = std::clamp(minX, 0, (int)width - 1);
            dirtyMinY = std::clamp(minY, 0, (int)height - 1);
            dirtyMaxX = std::clamp(maxX, 0, (int)width - 1);
            dirtyMaxY = std::clamp(maxY, 0, (int)height - 1);
        }
        isDirty = true;
    } else {
        dirtyMinX = std::max(0, std::min(dirtyMinX, minX));
        dirtyMinY = std::max(0, std::min(dirtyMinY, minY));
        dirtyMaxX = std::min((int)width - 1, std::max(dirtyMaxX, maxX));
        dirtyMaxY = std::min((int)height - 1, std::max(dirtyMaxY, maxY));
    }
}

void LayerStack::markFullDirty() {
    isDirty = true;
    fullDirty = true;
    dirtyMinX = 0;
    dirtyMinY = 0;
    dirtyMaxX = (int)width - 1;
    dirtyMaxY = (int)height - 1;
}

void LayerStack::composite() {
    compositeRect(0, 0, (int)width - 1, (int)height - 1);
    fullDirty = false;
    isDirty = false;
}

void LayerStack::compositeRect(int minX, int minY, int maxX, int maxY) {
    minX = std::max(0, minX);
    minY = std::max(0, minY);
    maxX = std::min((int)width - 1, maxX);
    maxY = std::min((int)height - 1, maxY);
    if (minX > maxX || minY > maxY) return;

    for (int y = minY; y <= maxY; ++y) {
        size_t rowOffset = (size_t)y * width;
        for (int x = minX; x <= maxX; ++x) {
            size_t idx = (rowOffset + x) * 4;

            // Default base environment color: neutral grey, medium roughness, non-metallic, flat normal
            compositedBaseColor[idx + 0] = 180;
            compositedBaseColor[idx + 1] = 180;
            compositedBaseColor[idx + 2] = 180;
            compositedBaseColor[idx + 3] = 255;

            compositedMetallicRoughness[idx + 0] = 255; // AO
            compositedMetallicRoughness[idx + 1] = 128; // Roughness 0.5
            compositedMetallicRoughness[idx + 2] = 0;   // Metallic 0.0
            compositedMetallicRoughness[idx + 3] = 255;

            compositedNormal[idx + 0] = 128; // Nx = 0
            compositedNormal[idx + 1] = 128; // Ny = 0
            compositedNormal[idx + 2] = 255; // Nz = 1
            compositedNormal[idx + 3] = 255;

            // Blend layers from bottom to top
            for (const auto& layer : layers) {
                if (!layer->visible || layer->opacity <= 0.001f || layer->type == LayerType::Folder) continue;

                float layerOpacity = layer->opacity;
                float srcA = (layer->colorData[idx + 3] / 255.0f) * layerOpacity;
                if (layer->type == LayerType::Fill) {
                    srcA = layerOpacity;
                }
                if (srcA <= 0.001f && layer->type == LayerType::Paint) continue;

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
    }
}

} // namespace REMoAINTER
