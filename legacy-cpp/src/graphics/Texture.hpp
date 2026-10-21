#pragma once

#include <vulkan/vulkan.h>
#include <vector>
#include <string>

namespace REMoAINTER {

class Texture {
public:
    uint32_t width = 0;
    uint32_t height = 0;
    VkFormat format = VK_FORMAT_R8G8B8A8_UNORM;

    VkImage image = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    VkImageView view = VK_NULL_HANDLE;
    VkSampler sampler = VK_NULL_HANDLE;

    VkDescriptorSet descriptorSet = VK_NULL_HANDLE;

    ~Texture();

    void create(VkDevice device, VkPhysicalDevice physicalDevice, uint32_t width, uint32_t height,
                VkFormat format, VkImageUsageFlags usage);
    void updateData(VkDevice device, VkPhysicalDevice physicalDevice, VkCommandPool commandPool,
                    VkQueue queue, const void* pixels, VkDeviceSize size);
    void destroy(VkDevice device);
};

} // namespace REMoAINTER
