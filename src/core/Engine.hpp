#pragma once

#define GLFW_INCLUDE_VULKAN
#include <GLFW/glfw3.h>
#include <vulkan/vulkan.h>

#include "Camera.hpp"
#include "../graphics/Mesh.hpp"
#include "../graphics/Texture.hpp"
#include "../painting/Brush.hpp"
#include "../painting/LayerStack.hpp"
#include "../ui/UIManager.hpp"

#include <vector>
#include <memory>

namespace REMoAINTER {

class Engine {
public:
    Engine(int width = 1600, int height = 900, const char* title = "REMoAINTER - 3D PBR Texture Painter");
    ~Engine();

    void run();

private:
    int windowWidth;
    int windowHeight;
    const char* windowTitle;

    GLFWwindow* window = nullptr;

    // Vulkan Core
    VkInstance instance = VK_NULL_HANDLE;
    VkDebugUtilsMessengerEXT debugMessenger = VK_NULL_HANDLE;
    VkSurfaceKHR surface = VK_NULL_HANDLE;
    VkPhysicalDevice physicalDevice = VK_NULL_HANDLE;
    VkDevice device = VK_NULL_HANDLE;
    VkQueue graphicsQueue = VK_NULL_HANDLE;
    VkQueue presentQueue = VK_NULL_HANDLE;

    // Swapchain
    VkSwapchainKHR swapchain = VK_NULL_HANDLE;
    std::vector<VkImage> swapchainImages;
    VkFormat swapchainImageFormat;
    VkExtent2D swapchainExtent;
    std::vector<VkImageView> swapchainImageViews;

    // Depth buffer
    VkImage depthImage = VK_NULL_HANDLE;
    VkDeviceMemory depthImageMemory = VK_NULL_HANDLE;
    VkImageView depthImageView = VK_NULL_HANDLE;

    // Render pass & Framebuffers
    VkRenderPass renderPass = VK_NULL_HANDLE;
    std::vector<VkFramebuffer> swapchainFramebuffers;

    // Command Pools & Buffers
    VkCommandPool commandPool = VK_NULL_HANDLE;
    std::vector<VkCommandBuffer> commandBuffers;

    // Synchronization
    static const int MAX_FRAMES_IN_FLIGHT = 2;
    std::vector<VkSemaphore> imageAvailableSemaphores;
    std::vector<VkSemaphore> renderFinishedSemaphores;
    std::vector<VkFence> inFlightFences;
    uint32_t currentFrame = 0;

    // Descriptor Pool & Sets for PBR Shaders
    VkDescriptorPool descriptorPool = VK_NULL_HANDLE;
    VkDescriptorSetLayout pbrDescriptorSetLayout = VK_NULL_HANDLE;
    VkDescriptorSet pbrDescriptorSet = VK_NULL_HANDLE;

    // Pipeline
    VkPipelineLayout pbrPipelineLayout = VK_NULL_HANDLE;
    VkPipeline pbrPipeline = VK_NULL_HANDLE;

    // ImGui Descriptor Pool
    VkDescriptorPool imguiDescriptorPool = VK_NULL_HANDLE;

    // Application state
    Camera camera;
    Brush brush;
    LayerStack layerStack;
    UIManager uiManager;

    Mesh currentMesh;
    int currentMeshType = 0;

    // Active PBR textures on GPU
    Texture baseColorTexture;
    Texture metallicRoughnessTexture;
    Texture normalTexture;

    // Mouse / Interaction tracking
    double lastMouseX = 0.0;
    double lastMouseY = 0.0;
    bool isPainting = false;
    bool isMeshHovered = false;
    glm::vec2 hoveredUV{0.0f};

    void initWindow();
    void initVulkan();
    void initImGui();
    void mainLoop();
    void cleanup();

    void createInstance();
    void setupDebugMessenger();
    void createSurface();
    void pickPhysicalDevice();
    void createLogicalDevice();
    void createSwapchain();
    void createImageViews();
    void createRenderPass();
    void createDepthResources();
    void createFramebuffers();
    void createCommandPool();
    void createCommandBuffers();
    void createSyncObjects();

    void createDescriptorSetLayout();
    void createDescriptorPool();
    void createPbrPipeline();
    void initTextures();
    void updatePbrTexturesFromLayerStack();
    void updateDescriptorSets();

    void recreateSwapchain();
    void cleanupSwapchain();

    void recordCommandBuffer(VkCommandBuffer commandBuffer, uint32_t imageIndex);
    void drawFrame();
    void processInput();

    void switchMesh(int type, const std::string& customPath = "");

    static void framebufferResizeCallback(GLFWwindow* window, int width, int height);
};

} // namespace REMoAINTER
