#pragma once

#include <string>
#include <vector>
#include <glm/glm.hpp>

namespace REMoAINTER {

struct MaterialPreset {
    std::string name;
    std::string category;
    glm::vec4 baseColor;
    float metallic;
    float roughness;
};

struct BrushPreset {
    std::string name;
    float size;
    float hardness;
    float opacity;
    float flow;
    int alphaIndex; // 0: None/Round, 1: Soft, 2: Noise/Grunge, 3: Scratches, 4: Stipple, 5: Square, 6: Star
};

struct AlphaPreset {
    std::string name;
    int type; // 0: RoundHard, 1: RoundSoft, 2: Noise, 3: Scratches, 4: Stipple, 5: Square, 6: Star
};

struct EnvironmentPreset {
    std::string name;
    int id;
    std::string description;
};

class PresetLibrary {
public:
    static std::vector<MaterialPreset> getMaterials() {
        return {
            // Metals
            { "Gold Pure", "Metals", glm::vec4(1.00f, 0.86f, 0.45f, 1.0f), 1.0f, 0.12f },
            { "Steel Polished", "Metals", glm::vec4(0.88f, 0.89f, 0.90f, 1.0f), 1.0f, 0.20f },
            { "Steel Rough", "Metals", glm::vec4(0.76f, 0.78f, 0.80f, 1.0f), 1.0f, 0.50f },
            { "Copper Satin", "Metals", glm::vec4(0.96f, 0.60f, 0.45f, 1.0f), 1.0f, 0.28f },
            { "Brass Brushed", "Metals", glm::vec4(0.85f, 0.72f, 0.38f, 1.0f), 1.0f, 0.35f },
            { "Aluminum Dull", "Metals", glm::vec4(0.82f, 0.84f, 0.86f, 1.0f), 0.95f, 0.42f },
            { "Iron Cast Dark", "Metals", glm::vec4(0.35f, 0.36f, 0.38f, 1.0f), 0.90f, 0.65f },

            // Automotive & Gloss
            { "Car Paint Red", "Automotive", glm::vec4(0.80f, 0.08f, 0.10f, 1.0f), 0.85f, 0.10f },
            { "Car Paint Blue", "Automotive", glm::vec4(0.06f, 0.30f, 0.75f, 1.0f), 0.90f, 0.08f },
            { "Car Paint Cyber Yellow", "Automotive", glm::vec4(0.95f, 0.75f, 0.05f, 1.0f), 0.80f, 0.12f },

            // Plastics & Non-Metals
            { "Plastic Glossy Black", "Plastics", glm::vec4(0.12f, 0.12f, 0.13f, 1.0f), 0.0f, 0.12f },
            { "Plastic Matte White", "Plastics", glm::vec4(0.94f, 0.94f, 0.96f, 1.0f), 0.0f, 0.55f },
            { "Plastic Cyber Orange", "Plastics", glm::vec4(0.92f, 0.42f, 0.08f, 1.0f), 0.0f, 0.25f },
            { "Rubber Charcoal", "Plastics", glm::vec4(0.18f, 0.18f, 0.20f, 1.0f), 0.0f, 0.85f },

            // Natural & Weathered
            { "Rusted Metal", "Weathered", glm::vec4(0.48f, 0.25f, 0.14f, 1.0f), 0.35f, 0.78f },
            { "Concrete Raw", "Weathered", glm::vec4(0.55f, 0.56f, 0.58f, 1.0f), 0.0f, 0.92f },
            { "Leather Dark", "Organic", glm::vec4(0.32f, 0.18f, 0.10f, 1.0f), 0.0f, 0.65f },
            { "Military Olive", "Fabrics", glm::vec4(0.30f, 0.34f, 0.22f, 1.0f), 0.0f, 0.75f },
            { "Ceramic Emerald", "Ceramics", glm::vec4(0.06f, 0.45f, 0.28f, 1.0f), 0.05f, 0.12f }
        };
    }

    static std::vector<BrushPreset> getBrushes() {
        return {
            { "Basic Hard", 32.0f, 0.98f, 1.0f, 1.0f, 0 },
            { "Basic Soft", 48.0f, 0.20f, 1.0f, 0.8f, 1 },
            { "Airbrush Flow", 64.0f, 0.02f, 0.5f, 0.35f, 1 },
            { "Detail Fine", 8.0f, 0.95f, 1.0f, 1.0f, 0 },
            { "Grunge Dirt", 45.0f, 0.60f, 0.8f, 0.65f, 2 },
            { "Metal Scratches", 35.0f, 0.85f, 0.9f, 0.75f, 3 },
            { "Stipple Noise", 50.0f, 0.50f, 0.7f, 0.55f, 4 },
            { "Hard Chisel", 28.0f, 1.00f, 1.0f, 1.0f, 5 }
        };
    }

    static std::vector<AlphaPreset> getAlphas() {
        return {
            { "Circle Hard", 0 },
            { "Circle Soft", 1 },
            { "Grunge Noise", 2 },
            { "Linear Scratches", 3 },
            { "Stipple Grid", 4 },
            { "Square Box", 5 },
            { "Star Burst", 6 }
        };
    }

    static std::vector<EnvironmentPreset> getEnvironments() {
        return {
            { "Studio Neutral", 0, "Clean high-contrast three-point studio lighting" },
            { "Sunset Warm", 1, "Golden hour low-angle sunlight with cool ambient sky" },
            { "Cyberpunk Neon", 2, "Vibrant cyan key and magenta rim lighting" },
            { "Forest Daylight", 3, "Natural sunlight filtered through green canopy" },
            { "Desert High Noon", 4, "Blazing overhead sun with warm ground bounce" }
        };
    }
};

} // namespace REMoAINTER
