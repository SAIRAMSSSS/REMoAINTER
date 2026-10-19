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

struct SmartMaterialLayerDef {
    std::string name;
    bool isFill;
    glm::vec4 color;
    float metallic;
    float roughness;
    float opacity;
    std::string blendMode; // "Norm", "Mult", "Screen", "Over", "Add"
};

struct SmartMaterialPreset {
    std::string name;
    std::string category;
    std::vector<SmartMaterialLayerDef> layers;
};

struct BrushPreset {
    std::string name;
    float size;
    float hardness;
    float opacity;
    float flow;
    float spacing;
    int alphaIndex;
};

struct AlphaPreset {
    std::string name;
    std::string category;
    int type; // 0: RoundHard, 1: RoundSoft, 2: Noise, 3: Scratches, 4: Stipple, 5: Square, 6: Star, 7: Biohazard, 8: HazardStripe, 9: TechDial
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
            // Authentic Substance Painter Asset Library Materials
            { "Aluminum Pure", "Metals", glm::vec4(0.91f, 0.92f, 0.93f, 1.0f), 1.0f, 0.18f },
            { "Aluminum Rough", "Metals", glm::vec4(0.82f, 0.83f, 0.85f, 1.0f), 1.0f, 0.48f },
            { "Artificial Leather", "Leather", glm::vec4(0.24f, 0.16f, 0.11f, 1.0f), 0.0f, 0.58f },
            { "Autumn Grass", "Organic", glm::vec4(0.35f, 0.38f, 0.14f, 1.0f), 0.0f, 0.85f },
            { "Baked Ceramic", "Ceramics", glm::vec4(0.86f, 0.82f, 0.76f, 1.0f), 0.05f, 0.22f },
            { "Brass Polished", "Metals", glm::vec4(0.95f, 0.78f, 0.36f, 1.0f), 1.0f, 0.15f },
            { "Calf Skin", "Leather", glm::vec4(0.42f, 0.28f, 0.19f, 1.0f), 0.0f, 0.62f },
            { "Carbon Fiber", "Synthetics", glm::vec4(0.12f, 0.12f, 0.13f, 1.0f), 0.15f, 0.25f },
            { "Coated Metal", "Metals", glm::vec4(0.28f, 0.38f, 0.44f, 1.0f), 0.80f, 0.20f },
            { "Cobalt Pure", "Metals", glm::vec4(0.66f, 0.67f, 0.68f, 1.0f), 1.0f, 0.22f },
            { "Concrete Bare", "Architectural", glm::vec4(0.52f, 0.53f, 0.55f, 1.0f), 0.0f, 0.88f },
            { "Concrete Clean", "Architectural", glm::vec4(0.65f, 0.66f, 0.68f, 1.0f), 0.0f, 0.72f },
            { "Concrete Rough", "Architectural", glm::vec4(0.44f, 0.45f, 0.47f, 1.0f), 0.0f, 0.95f },
            { "Copper Pure", "Metals", glm::vec4(0.97f, 0.64f, 0.53f, 1.0f), 1.0f, 0.16f },
            { "Copper Rust", "Weathered", glm::vec4(0.28f, 0.56f, 0.48f, 1.0f), 0.25f, 0.75f },
            { "Fabric Denim", "Fabrics", glm::vec4(0.18f, 0.26f, 0.42f, 1.0f), 0.0f, 0.82f },
            { "Fabric Knit", "Fabrics", glm::vec4(0.58f, 0.22f, 0.25f, 1.0f), 0.0f, 0.90f },
            { "Fabric Plain", "Fabrics", glm::vec4(0.72f, 0.70f, 0.68f, 1.0f), 0.0f, 0.80f },
            { "Fabric Rough", "Fabrics", glm::vec4(0.38f, 0.35f, 0.30f, 1.0f), 0.0f, 0.88f },
            { "Fabric Satin", "Fabrics", glm::vec4(0.78f, 0.14f, 0.22f, 1.0f), 0.05f, 0.28f },
            { "Fabric Velvet", "Fabrics", glm::vec4(0.15f, 0.12f, 0.35f, 1.0f), 0.0f, 0.70f },
            { "Fingerprints", "Smart Masks", glm::vec4(0.85f, 0.85f, 0.85f, 1.0f), 0.0f, 0.40f },
            { "Gold Pure", "Metals", glm::vec4(1.00f, 0.85f, 0.42f, 1.0f), 1.0f, 0.12f },
            { "Gouache Paint", "Artistic", glm::vec4(0.10f, 0.65f, 0.85f, 1.0f), 0.0f, 0.45f },
            { "Greased Metal", "Weathered", glm::vec4(0.32f, 0.34f, 0.36f, 1.0f), 0.85f, 0.32f },
            { "Skin Human", "Organic", glm::vec4(0.88f, 0.65f, 0.55f, 1.0f), 0.0f, 0.48f },
            { "Wood Oak", "Wood", glm::vec4(0.55f, 0.38f, 0.22f, 1.0f), 0.0f, 0.65f },
            { "Wood Walnut", "Wood", glm::vec4(0.32f, 0.20f, 0.12f, 1.0f), 0.0f, 0.58f },
            { "Car Paint Red", "Automotive", glm::vec4(0.82f, 0.06f, 0.08f, 1.0f), 0.85f, 0.08f },
            { "Plastic Glossy Black", "Plastics", glm::vec4(0.08f, 0.08f, 0.09f, 1.0f), 0.0f, 0.10f },
            { "Plastic Matte White", "Plastics", glm::vec4(0.95f, 0.95f, 0.96f, 1.0f), 0.0f, 0.55f }
        };
    }

    static std::vector<SmartMaterialPreset> getSmartMaterials() {
        return {
            {
                "Steel Painted Green", "Metal",
                {
                    { "Cast Steel Substrate", true, glm::vec4(0.35f, 0.36f, 0.38f, 1.0f), 0.95f, 0.40f, 1.0f, "Norm" },
                    { "Primer Coat Grey", true, glm::vec4(0.28f, 0.28f, 0.30f, 1.0f), 0.0f, 0.65f, 1.0f, "Norm" },
                    { "Machine Green Enamel", true, glm::vec4(0.14f, 0.45f, 0.28f, 1.0f), 0.0f, 0.35f, 1.0f, "Norm" },
                    { "Edge Scratches & Chips", false, glm::vec4(0.75f, 0.75f, 0.78f, 1.0f), 1.0f, 0.22f, 0.95f, "Norm" },
                    { "Crevice Dirt & Dust", true, glm::vec4(0.10f, 0.08f, 0.06f, 1.0f), 0.05f, 0.92f, 0.45f, "Mult" }
                }
            },
            {
                "Aluminium Anodized Red", "Metal",
                {
                    { "Aluminum Base", true, glm::vec4(0.90f, 0.91f, 0.92f, 1.0f), 1.0f, 0.22f, 1.0f, "Norm" },
                    { "Red Anodized Sheen", true, glm::vec4(0.85f, 0.06f, 0.10f, 1.0f), 0.85f, 0.16f, 0.95f, "Over" },
                    { "Specular Edge Highlights", false, glm::vec4(0.96f, 0.96f, 0.98f, 1.0f), 1.0f, 0.12f, 0.85f, "Screen" }
                }
            },
            {
                "Bronze Corroded Statue", "Metal",
                {
                    { "Cast Antique Bronze", true, glm::vec4(0.62f, 0.44f, 0.24f, 1.0f), 1.0f, 0.32f, 1.0f, "Norm" },
                    { "Verdigris Patina Crust", true, glm::vec4(0.22f, 0.65f, 0.54f, 1.0f), 0.05f, 0.88f, 0.82f, "Norm" },
                    { "Polished Highpoints", false, glm::vec4(0.82f, 0.62f, 0.34f, 1.0f), 1.0f, 0.16f, 0.90f, "Norm" }
                }
            },
            {
                "Gold Damaged 24K", "Metal",
                {
                    { "Pure 24K Gold Base", true, glm::vec4(1.00f, 0.85f, 0.42f, 1.0f), 1.0f, 0.12f, 1.0f, "Norm" },
                    { "Surface Micro-Scratches", false, glm::vec4(0.82f, 0.68f, 0.30f, 1.0f), 0.90f, 0.45f, 0.70f, "Norm" },
                    { "Crevice Cavity Dirt", true, glm::vec4(0.12f, 0.09f, 0.04f, 1.0f), 0.0f, 0.95f, 0.40f, "Mult" }
                }
            },
            {
                "Machinery Heavy Duty", "Metal",
                {
                    { "Cast Iron Frame", true, glm::vec4(0.22f, 0.23f, 0.24f, 1.0f), 0.95f, 0.50f, 1.0f, "Norm" },
                    { "Safety Yellow Enamel", true, glm::vec4(0.95f, 0.78f, 0.08f, 1.0f), 0.0f, 0.28f, 1.0f, "Norm" },
                    { "Oil & Grease Leaks", false, glm::vec4(0.04f, 0.04f, 0.05f, 1.0f), 0.20f, 0.06f, 0.80f, "Norm" },
                    { "Surface Rust Spots", true, glm::vec4(0.42f, 0.18f, 0.08f, 1.0f), 0.25f, 0.85f, 0.40f, "Over" }
                }
            },
            {
                "Leather Calf Aged", "Leather",
                {
                    { "Cognac Leather Base", true, glm::vec4(0.38f, 0.22f, 0.14f, 1.0f), 0.0f, 0.55f, 1.0f, "Norm" },
                    { "Deep Crease Grain", true, glm::vec4(0.24f, 0.13f, 0.08f, 1.0f), 0.0f, 0.68f, 0.65f, "Mult" },
                    { "Surface Worn Sheen", false, glm::vec4(0.52f, 0.35f, 0.24f, 1.0f), 0.0f, 0.32f, 0.50f, "Norm" }
                }
            },
            {
                "Hull Damage Sci-Fi", "Synthetics",
                {
                    { "Ceramic Armor Plating", true, glm::vec4(0.85f, 0.86f, 0.88f, 1.0f), 0.10f, 0.35f, 1.0f, "Norm" },
                    { "Exposed Composite Core", true, glm::vec4(0.10f, 0.10f, 0.11f, 1.0f), 0.40f, 0.70f, 0.80f, "Norm" },
                    { "Thermal Blast Burn", false, glm::vec4(0.05f, 0.04f, 0.03f, 1.0f), 0.0f, 0.95f, 0.70f, "Mult" }
                }
            },
            {
                "Marble Carrara Polished", "Stone",
                {
                    { "White Marble Base", true, glm::vec4(0.92f, 0.93f, 0.94f, 1.0f), 0.0f, 0.18f, 1.0f, "Norm" },
                    { "Mineral Veins Grey", false, glm::vec4(0.45f, 0.48f, 0.50f, 1.0f), 0.0f, 0.25f, 0.65f, "Norm" },
                    { "Polished Clearcoat", true, glm::vec4(1.0f, 1.0f, 1.0f, 1.0f), 0.0f, 0.05f, 0.35f, "Screen" }
                }
            }
        };
    }

    static std::vector<BrushPreset> getBrushes() {
        return {
            { "Basic Paint", 32.0f, 0.85f, 1.0f, 1.0f, 0.12f, 0 },
            { "Soft Airbrush", 48.0f, 0.15f, 0.6f, 0.40f, 0.10f, 1 },
            { "Hard Chisel", 24.0f, 1.00f, 1.0f, 1.0f, 0.08f, 5 },
            { "Grunge Dirt", 45.0f, 0.60f, 0.8f, 0.65f, 0.15f, 2 },
            { "Metal Scratches", 30.0f, 0.90f, 0.9f, 0.80f, 0.18f, 3 },
            { "Stipple Noise", 50.0f, 0.50f, 0.7f, 0.55f, 0.20f, 4 },
            { "Star Burst", 36.0f, 0.75f, 0.85f, 0.70f, 0.25f, 6 },
            { "Detail Ink 4px", 4.0f, 0.95f, 1.0f, 1.0f, 0.05f, 0 }
        };
    }

    static std::vector<AlphaPreset> getAlphas() {
        return {
            { "Round Hard", "Basic", 0 },
            { "Round Soft", "Basic", 1 },
            { "Grunge Noise", "Noise", 2 },
            { "Linear Scratches", "Noise", 3 },
            { "Stipple Grid", "Noise", 4 },
            { "Square Box", "Hard Surface", 5 },
            { "Star Emblem", "Logos", 6 },
            { "Biohazard Logo", "Logos", 7 },
            { "Industrial Chevrons", "Hard Surface", 8 },
            { "Tech Dial Gauge", "Hard Surface", 9 }
        };
    }

    static std::vector<EnvironmentPreset> getEnvironments() {
        return {
            { "Warm & Cool Studio", 1, "Warm key light, cool soft fill, crisp rim and table bounce (Reference setup)" },
            { "Studio Neutral", 0, "Clean 5500K balanced three-point photography studio" },
            { "Cyberpunk Neon", 2, "Vibrant cyan key and magenta rim lighting with dark ground" },
            { "Forest Daylight", 3, "Natural midday sunlight with green foliage bounce" },
            { "Desert High Noon", 4, "High-intensity noon sun with warm sand reflection" }
        };
    }
};

} // namespace REMoAINTER
