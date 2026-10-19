#include "Layer.hpp"
#include <algorithm>
#include <cmath>
#include <cstring>

namespace REMoAINTER {

Layer::Layer(const std::string& n, LayerType t, uint32_t w, uint32_t h)
    : name(n), type(t), width(w), height(h) {
    resize(width, height);
}

void Layer::resize(uint32_t w, uint32_t h) {
    width = w;
    height = h;
    size_t numPixels = (size_t)width * height;

    colorData.assign(numPixels * 4, 0);
    mrData.assign(numPixels * 4, 0);
    normalData.assign(numPixels * 4, 0);

    clear();
}

void Layer::clear() {
    size_t numPixels = (size_t)width * height;
    if (type == LayerType::Fill) {
        // Pre-fill buffer with fill parameters
        uint8_t cr = (uint8_t)std::clamp(fillColor.r * 255.0f, 0.0f, 255.0f);
        uint8_t cg = (uint8_t)std::clamp(fillColor.g * 255.0f, 0.0f, 255.0f);
        uint8_t cb = (uint8_t)std::clamp(fillColor.b * 255.0f, 0.0f, 255.0f);
        uint8_t ca = (uint8_t)std::clamp(fillColor.a * 255.0f, 0.0f, 255.0f);

        uint8_t rough = (uint8_t)std::clamp(fillRoughness * 255.0f, 0.0f, 255.0f);
        uint8_t metal = (uint8_t)std::clamp(fillMetallic * 255.0f, 0.0f, 255.0f);

        uint8_t nx = (uint8_t)std::clamp((fillNormal.x * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
        uint8_t ny = (uint8_t)std::clamp((fillNormal.y * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
        uint8_t nz = (uint8_t)std::clamp((fillNormal.z * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);

        for (size_t i = 0; i < numPixels; ++i) {
            colorData[i * 4 + 0] = cr;
            colorData[i * 4 + 1] = cg;
            colorData[i * 4 + 2] = cb;
            colorData[i * 4 + 3] = ca;

            mrData[i * 4 + 0] = 255;   // AO
            mrData[i * 4 + 1] = rough; // Roughness
            mrData[i * 4 + 2] = metal; // Metallic
            mrData[i * 4 + 3] = 255;   // Coverage mask

            normalData[i * 4 + 0] = nx;
            normalData[i * 4 + 1] = ny;
            normalData[i * 4 + 2] = nz;
            normalData[i * 4 + 3] = 128; // Flat height
        }
    } else {
        // Paint layer starts with 0 alpha (transparent)
        for (size_t i = 0; i < numPixels; ++i) {
            colorData[i * 4 + 0] = 0;
            colorData[i * 4 + 1] = 0;
            colorData[i * 4 + 2] = 0;
            colorData[i * 4 + 3] = 0; // Alpha 0

            mrData[i * 4 + 0] = 255;
            mrData[i * 4 + 1] = 128;
            mrData[i * 4 + 2] = 0;
            mrData[i * 4 + 3] = 0; // Coverage 0

            normalData[i * 4 + 0] = 128;
            normalData[i * 4 + 1] = 128;
            normalData[i * 4 + 2] = 255;
            normalData[i * 4 + 3] = 128;
        }
    }
}

void Layer::paintStamp(float uvX, float uvY, const Brush& brush) {
    if (type != LayerType::Paint) return;

    // Convert UV to pixel center
    float px = uvX * (float)width;
    float py = uvY * (float)height;

    float r = brush.size;
    int minX = std::max(0, (int)std::floor(px - r));
    int maxX = std::min((int)width - 1, (int)std::ceil(px + r));
    int minY = std::max(0, (int)std::floor(py - r));
    int maxY = std::min((int)height - 1, (int)std::ceil(py + r));

    float r2 = r * r;
    float hardness = std::clamp(brush.hardness, 0.01f, 0.999f);

    uint8_t brushColR = (uint8_t)std::clamp(brush.color.r * 255.0f, 0.0f, 255.0f);
    uint8_t brushColG = (uint8_t)std::clamp(brush.color.g * 255.0f, 0.0f, 255.0f);
    uint8_t brushColB = (uint8_t)std::clamp(brush.color.b * 255.0f, 0.0f, 255.0f);
    uint8_t brushRough = (uint8_t)std::clamp(brush.roughness * 255.0f, 0.0f, 255.0f);
    uint8_t brushMetal = (uint8_t)std::clamp(brush.metallic * 255.0f, 0.0f, 255.0f);

    uint8_t brushNx = (uint8_t)std::clamp((brush.normal.x * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
    uint8_t brushNy = (uint8_t)std::clamp((brush.normal.y * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
    uint8_t brushNz = (uint8_t)std::clamp((brush.normal.z * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);

    for (int y = minY; y <= maxY; ++y) {
        float dy = (float)y - py;
        float dy2 = dy * dy;

        for (int x = minX; x <= maxX; ++x) {
            float dx = (float)x - px;
            float dist2 = dx * dx + dy2;

            if (dist2 <= r2) {
                float dist = std::sqrt(dist2) / r; // 0.0 at center, 1.0 at border

                // Calculate falloff curve based on hardness
                float falloff;
                if (dist <= hardness) {
                    falloff = 1.0f;
                } else {
                    float t = (dist - hardness) / (1.0f - hardness);
                    falloff = 0.5f * (1.0f + std::cos(t * 3.14159265f)); // Smooth cosine falloff
                }

                float alpha = falloff * brush.opacity * brush.flow;
                if (alpha <= 0.001f) continue;

                size_t idx = ((size_t)y * width + x) * 4;

                if (brush.isEraser) {
                    float existingAlpha = colorData[idx + 3] / 255.0f;
                    float newAlpha = std::max(0.0f, existingAlpha - alpha);
                    colorData[idx + 3] = (uint8_t)(newAlpha * 255.0f);
                    mrData[idx + 3] = (uint8_t)(newAlpha * 255.0f);
                    continue;
                }

                // Blend Color
                if (brush.paintColor) {
                    float existingAlpha = colorData[idx + 3] / 255.0f;
                    float outAlpha = alpha + existingAlpha * (1.0f - alpha);

                    if (outAlpha > 0.001f) {
                        float rVal = (brushColR * alpha + colorData[idx + 0] * existingAlpha * (1.0f - alpha)) / outAlpha;
                        float gVal = (brushColG * alpha + colorData[idx + 1] * existingAlpha * (1.0f - alpha)) / outAlpha;
                        float bVal = (brushColB * alpha + colorData[idx + 2] * existingAlpha * (1.0f - alpha)) / outAlpha;

                        colorData[idx + 0] = (uint8_t)std::clamp(rVal, 0.0f, 255.0f);
                        colorData[idx + 1] = (uint8_t)std::clamp(gVal, 0.0f, 255.0f);
                        colorData[idx + 2] = (uint8_t)std::clamp(bVal, 0.0f, 255.0f);
                        colorData[idx + 3] = (uint8_t)std::clamp(outAlpha * 255.0f, 0.0f, 255.0f);
                    }
                }

                // Blend Metallic / Roughness
                if (brush.paintRoughness || brush.paintMetallic) {
                    float existingCoverage = mrData[idx + 3] / 255.0f;
                    float outCoverage = alpha + existingCoverage * (1.0f - alpha);

                    if (brush.paintRoughness) {
                        float rVal = (brushRough * alpha + mrData[idx + 1] * existingCoverage * (1.0f - alpha)) / std::max(outCoverage, 0.001f);
                        mrData[idx + 1] = (uint8_t)std::clamp(rVal, 0.0f, 255.0f);
                    }
                    if (brush.paintMetallic) {
                        float mVal = (brushMetal * alpha + mrData[idx + 2] * existingCoverage * (1.0f - alpha)) / std::max(outCoverage, 0.001f);
                        mrData[idx + 2] = (uint8_t)std::clamp(mVal, 0.0f, 255.0f);
                    }
                    mrData[idx + 3] = (uint8_t)std::clamp(outCoverage * 255.0f, 0.0f, 255.0f);
                }

                // Blend Normal
                if (brush.paintNormal) {
                    normalData[idx + 0] = (uint8_t)(brushNx * alpha + normalData[idx + 0] * (1.0f - alpha));
                    normalData[idx + 1] = (uint8_t)(brushNy * alpha + normalData[idx + 1] * (1.0f - alpha));
                    normalData[idx + 2] = (uint8_t)(brushNz * alpha + normalData[idx + 2] * (1.0f - alpha));
                }
            }
        }
    }
}

} // namespace REMoAINTER
