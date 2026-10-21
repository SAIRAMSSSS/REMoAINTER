#pragma once

#include "../painting/LayerStack.hpp"
#include <string>

namespace REMoAINTER {

class TextureExporter {
public:
    static bool exportPBRMaps(const LayerStack& stack, const std::string& outputDir, const std::string& prefix = "Material");
};

} // namespace REMoAINTER
