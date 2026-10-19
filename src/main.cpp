#include "core/Engine.hpp"
#include <iostream>

int main(int argc, char** argv) {
    std::cout << "====================================================\n";
    std::cout << "  REMoAINTER - Open Source 3D PBR Texture Painter  \n";
    std::cout << "  Cross-Platform Vulkan 1.3 Architecture            \n";
    std::cout << "====================================================\n";

    try {
        REMoAINTER::Engine app(1600, 900, "REMoAINTER - 3D PBR Texture Painter");
        app.run();
    } catch (const std::exception& e) {
        std::cerr << "[CRITICAL ERROR]: " << e.what() << std::endl;
        return 1;
    }

    return 0;
}
