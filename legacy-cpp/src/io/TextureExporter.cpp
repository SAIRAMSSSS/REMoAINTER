#include "TextureExporter.hpp"

#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "../../third_party/stb/stb_image_write.h"

#include <iostream>
#include <vector>

namespace REMoAINTER {

bool TextureExporter::exportPBRMaps(const LayerStack& stack, const std::string& outputDir, const std::string& prefix) {
    uint32_t w = stack.width;
    uint32_t h = stack.height;
    size_t numPixels = (size_t)w * h;

    std::string baseColorPath = outputDir + "/" + prefix + "_BaseColor.png";
    std::string normalPath = outputDir + "/" + prefix + "_Normal.png";
    std::string roughnessPath = outputDir + "/" + prefix + "_Roughness.png";
    std::string metallicPath = outputDir + "/" + prefix + "_Metallic.png";
    std::string metallicRoughnessPath = outputDir + "/" + prefix + "_MetallicRoughness.png";

    // 1. Export Base Color (RGB / RGBA)
    if (!stbi_write_png(baseColorPath.c_str(), w, h, 4, stack.compositedBaseColor.data(), w * 4)) {
        std::cerr << "Failed to write " << baseColorPath << std::endl;
        return false;
    }

    // 2. Export Normal Map (RGB)
    std::vector<uint8_t> normalRGB(numPixels * 3);
    for (size_t i = 0; i < numPixels; ++i) {
        normalRGB[i * 3 + 0] = stack.compositedNormal[i * 4 + 0];
        normalRGB[i * 3 + 1] = stack.compositedNormal[i * 4 + 1];
        normalRGB[i * 3 + 2] = stack.compositedNormal[i * 4 + 2];
    }
    stbi_write_png(normalPath.c_str(), w, h, 3, normalRGB.data(), w * 3);

    // 3. Export Roughness Map (Grayscale)
    std::vector<uint8_t> roughnessGray(numPixels);
    for (size_t i = 0; i < numPixels; ++i) {
        roughnessGray[i] = stack.compositedMetallicRoughness[i * 4 + 1];
    }
    stbi_write_png(roughnessPath.c_str(), w, h, 1, roughnessGray.data(), w);

    // 4. Export Metallic Map (Grayscale)
    std::vector<uint8_t> metallicGray(numPixels);
    for (size_t i = 0; i < numPixels; ++i) {
        metallicGray[i] = stack.compositedMetallicRoughness[i * 4 + 2];
    }
    stbi_write_png(metallicPath.c_str(), w, h, 1, metallicGray.data(), w);

    // 5. Export glTF Standard Packed MetallicRoughness (G: Roughness, B: Metallic)
    stbi_write_png(metallicRoughnessPath.c_str(), w, h, 4, stack.compositedMetallicRoughness.data(), w * 4);

    std::cout << "Successfully exported PBR maps to " << outputDir << std::endl;
    return true;
}

} // namespace REMoAINTER
