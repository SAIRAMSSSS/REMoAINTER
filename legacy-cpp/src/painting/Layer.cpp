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
            normalData[i * 4 + 3] = 128;
        }
    } else {
        for (size_t i = 0; i < numPixels; ++i) {
            colorData[i * 4 + 0] = 0;
            colorData[i * 4 + 1] = 0;
            colorData[i * 4 + 2] = 0;
            colorData[i * 4 + 3] = 0;

            mrData[i * 4 + 0] = 255;
            mrData[i * 4 + 1] = 128;
            mrData[i * 4 + 2] = 0;
            mrData[i * 4 + 3] = 0;

            normalData[i * 4 + 0] = 128;
            normalData[i * 4 + 1] = 128;
            normalData[i * 4 + 2] = 255;
            normalData[i * 4 + 3] = 128;
        }
    }
}

// Procedural Hash for Noise Alphas
static inline float hash2D(float x, float y) {
    float n = std::sin(x * 12.9898f + y * 78.233f) * 43758.5453f;
    return n - std::floor(n);
}

void Layer::paintStamp(float uvX, float uvY, const Brush& brush,
                       int* outMinX, int* outMinY, int* outMaxX, int* outMaxY) {
    if (type != LayerType::Paint) return;

    int totalMinX = (int)width;
    int totalMinY = (int)height;
    int totalMaxX = -1;
    int totalMaxY = -1;

    // Normalize and wrap UV
    uvX = uvX - std::floor(uvX);
    uvY = std::clamp(uvY, 0.0f, 1.0f);

    float px = uvX * (float)width;
    float py = uvY * (float)height;

    auto applyStampAt = [this, &brush, &totalMinX, &totalMinY, &totalMaxX, &totalMaxY](float cx, float cy) {
        float r = brush.size;
        int minX = std::max(0, (int)std::floor(cx - r));
        int maxX = std::min((int)width - 1, (int)std::ceil(cx + r));
        int minY = std::max(0, (int)std::floor(cy - r));
        int maxY = std::min((int)height - 1, (int)std::ceil(cy + r));

        if (minX > maxX || minY > maxY) return;

        totalMinX = std::min(totalMinX, minX);
        totalMinY = std::min(totalMinY, minY);
        totalMaxX = std::max(totalMaxX, maxX);
        totalMaxY = std::max(totalMaxY, maxY);

        float hardness = std::clamp(brush.hardness, 0.01f, 0.999f);

        uint8_t brushColR = (uint8_t)std::clamp(brush.color.r * 255.0f, 0.0f, 255.0f);
        uint8_t brushColG = (uint8_t)std::clamp(brush.color.g * 255.0f, 0.0f, 255.0f);
        uint8_t brushColB = (uint8_t)std::clamp(brush.color.b * 255.0f, 0.0f, 255.0f);
        uint8_t brushRough = (uint8_t)std::clamp(brush.roughness * 255.0f, 0.0f, 255.0f);
        uint8_t brushMetal = (uint8_t)std::clamp(brush.metallic * 255.0f, 0.0f, 255.0f);

        uint8_t brushNx = (uint8_t)std::clamp((brush.normal.x * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
        uint8_t brushNy = (uint8_t)std::clamp((brush.normal.y * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);
        uint8_t brushNz = (uint8_t)std::clamp((brush.normal.z * 0.5f + 0.5f) * 255.0f, 0.0f, 255.0f);

        float cosAngle = std::cos(brush.angle);
        float sinAngle = std::sin(brush.angle);

        for (int y = minY; y <= maxY; ++y) {
            float dy = (float)y - cy;

            for (int x = minX; x <= maxX; ++x) {
                float dx = (float)x - cx;

                float rx = (dx * cosAngle - dy * sinAngle) / r;
                float ry = (dx * sinAngle + dy * cosAngle) / r;
                float dist = std::sqrt(rx * rx + ry * ry);

                if (dist > 1.0f && brush.alphaType != 5) continue;

                float alphaMask = 1.0f;
                switch (brush.alphaType) {
                    case 1:
                        alphaMask = std::pow(std::clamp(1.0f - dist, 0.0f, 1.0f), 1.5f);
                        break;
                    case 2: {
                        float baseFalloff = (dist <= hardness) ? 1.0f : 0.5f * (1.0f + std::cos((dist - hardness) / (1.0f - hardness) * 3.14159f));
                        float n = hash2D(std::floor(rx * 20.0f), std::floor(ry * 20.0f));
                        alphaMask = baseFalloff * (n > 0.35f ? 1.0f : 0.0f);
                        break;
                    }
                    case 3: {
                        float baseFalloff = (dist <= hardness) ? 1.0f : 0.5f * (1.0f + std::cos((dist - hardness) / (1.0f - hardness) * 3.14159f));
                        float scratch = std::abs(std::sin(rx * 18.0f + ry * 3.0f));
                        alphaMask = baseFalloff * (scratch > 0.5f ? 1.0f : 0.05f);
                        break;
                    }
                    case 4: {
                        float baseFalloff = (dist <= hardness) ? 1.0f : 0.5f * (1.0f + std::cos((dist - hardness) / (1.0f - hardness) * 3.14159f));
                        float dots = std::sin(rx * 35.0f) * std::sin(ry * 35.0f);
                        alphaMask = baseFalloff * (dots > 0.0f ? 1.0f : 0.0f);
                        break;
                    }
                    case 5:
                        if (std::abs(rx) > 0.8f || std::abs(ry) > 0.8f) alphaMask = 0.0f;
                        else alphaMask = 1.0f;
                        break;
                    case 6: {
                        float starDist = std::abs(rx) + std::abs(ry);
                        if (starDist > 1.0f) alphaMask = 0.0f;
                        else alphaMask = 1.0f - starDist;
                        break;
                    }
                    case 7: { // Biohazard Decal Logo
                        float rDist = std::sqrt(rx * rx + ry * ry);
                        float angle = std::atan2(ry, rx);
                        float triple = std::fmod(angle + 3.14159f, 2.0944f) - 1.0472f;
                        bool ring = (rDist > 0.45f && rDist < 0.75f && std::abs(triple) > 0.25f);
                        bool centerDot = (rDist < 0.20f);
                        alphaMask = (ring || centerDot) ? 1.0f : 0.0f;
                        break;
                    }
                    case 8: { // Industrial Chevrons
                        float chevron = std::abs(rx) + ry * 0.7f;
                        float stripe = std::fmod(std::abs(chevron * 6.0f), 2.0f);
                        alphaMask = (stripe > 1.0f && std::abs(rx) < 0.85f && std::abs(ry) < 0.85f) ? 1.0f : 0.0f;
                        break;
                    }
                    case 9: { // Tech Dial Gauge
                        float rDist = std::sqrt(rx * rx + ry * ry);
                        float angle = std::atan2(ry, rx);
                        bool outerRing = (rDist > 0.70f && rDist < 0.85f);
                        bool innerTicks = (rDist > 0.50f && rDist < 0.70f && std::abs(std::sin(angle * 12.0f)) > 0.85f);
                        alphaMask = (outerRing || innerTicks) ? 1.0f : 0.0f;
                        break;
                    }
                    case 0:
                    default:
                        if (dist <= hardness) {
                            alphaMask = 1.0f;
                        } else {
                            float t = (dist - hardness) / (1.0f - hardness);
                            alphaMask = 0.5f * (1.0f + std::cos(t * 3.14159265f));
                        }
                        break;
                }

                float alpha = alphaMask * brush.opacity * brush.flow;
                if (alpha <= 0.001f) continue;

                size_t idx = ((size_t)y * width + x) * 4;

                if (brush.isEraser) {
                    float existingAlpha = colorData[idx + 3] / 255.0f;
                    float newAlpha = std::max(0.0f, existingAlpha - alpha);
                    colorData[idx + 3] = (uint8_t)(newAlpha * 255.0f);
                    mrData[idx + 3] = (uint8_t)(newAlpha * 255.0f);
                    continue;
                }

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

                if (brush.paintNormal) {
                    normalData[idx + 0] = (uint8_t)(brushNx * alpha + normalData[idx + 0] * (1.0f - alpha));
                    normalData[idx + 1] = (uint8_t)(brushNy * alpha + normalData[idx + 1] * (1.0f - alpha));
                    normalData[idx + 2] = (uint8_t)(brushNz * alpha + normalData[idx + 2] * (1.0f - alpha));
                }
            }
        }
    };

    applyStampAt(px, py);

    // Toroidal horizontal wrap across UV seam:
    if (px - brush.size < 0.0f) {
        applyStampAt(px + (float)width, py);
    }
    if (px + brush.size >= (float)width) {
        applyStampAt(px - (float)width, py);
    }

    if (outMinX && totalMinX <= totalMaxX) *outMinX = totalMinX;
    if (outMinY && totalMinY <= totalMaxY) *outMinY = totalMinY;
    if (outMaxX && totalMinX <= totalMaxX) *outMaxX = totalMaxX;
    if (outMaxY && totalMinY <= totalMaxY) *outMaxY = totalMaxY;
}

void Layer::paintLine(const glm::vec2& p0, const glm::vec2& p1, const Brush& brush,
                      int* outMinX, int* outMinY, int* outMaxX, int* outMaxY) {
    if (type != LayerType::Paint) return;

    int lineMinX = (int)width;
    int lineMinY = (int)height;
    int lineMaxX = -1;
    int lineMaxY = -1;

    auto updateBounds = [&](int sMinX, int sMinY, int sMaxX, int sMaxY) {
        if (sMinX <= sMaxX && sMinY <= sMaxY) {
            lineMinX = std::min(lineMinX, sMinX);
            lineMinY = std::min(lineMinY, sMinY);
            lineMaxX = std::max(lineMaxX, sMaxX);
            lineMaxY = std::max(lineMaxY, sMaxY);
        }
    };

    // Handle horizontal UV seam wrapping: compute shortest path across [0, 1)
    glm::vec2 targetP1 = p1;
    float dx = targetP1.x - p0.x;
    if (dx > 0.5f) {
        targetP1.x -= 1.0f;
    } else if (dx < -0.5f) {
        targetP1.x += 1.0f;
    }

    glm::vec2 startPx(p0.x * (float)width, p0.y * (float)height);
    glm::vec2 endPx(targetP1.x * (float)width, targetP1.y * (float)height);

    float distPx = glm::distance(startPx, endPx);
    float stepPx = std::max(1.0f, brush.size * std::max(brush.spacing, 0.05f));

    Brush currentBrush = brush;
    if (brush.followPath && distPx > 1.0f) {
        glm::vec2 dir = glm::normalize(endPx - startPx);
        currentBrush.angle = std::atan2(dir.y, dir.x);
    }

    int sMinX = (int)width, sMinY = (int)height, sMaxX = -1, sMaxY = -1;

    if (distPx < stepPx) {
        paintStamp(targetP1.x, targetP1.y, currentBrush, &sMinX, &sMinY, &sMaxX, &sMaxY);
        updateBounds(sMinX, sMinY, sMaxX, sMaxY);
    } else {
        int steps = static_cast<int>(std::ceil(distPx / stepPx));
        steps = std::min(steps, 600);
        for (int i = 1; i <= steps; ++i) {
            float t = static_cast<float>(i) / static_cast<float>(steps);
            glm::vec2 pt = glm::mix(p0, targetP1, t);
            paintStamp(pt.x, pt.y, currentBrush, &sMinX, &sMinY, &sMaxX, &sMaxY);
            updateBounds(sMinX, sMinY, sMaxX, sMaxY);
        }
    }

    if (outMinX && lineMinX <= lineMaxX) *outMinX = lineMinX;
    if (outMinY && lineMinY <= lineMaxY) *outMinY = lineMinY;
    if (outMaxX && lineMinX <= lineMaxX) *outMaxX = lineMaxX;
    if (outMaxY && lineMinY <= lineMaxY) *outMaxY = lineMaxY;
}

} // namespace REMoAINTER

