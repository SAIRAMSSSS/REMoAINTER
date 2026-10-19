#pragma once

#include <vector>
#include <string>
#include <vulkan/vulkan.h>
#define GLM_ENABLE_EXPERIMENTAL
#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>

namespace REMoAINTER {

struct Vertex {
    glm::vec3 position;
    glm::vec3 normal;
    glm::vec2 texCoord;
    glm::vec3 tangent;
    glm::vec3 bitangent;

    static VkVertexInputBindingDescription getBindingDescription() {
        VkVertexInputBindingDescription bindingDescription{};
        bindingDescription.binding = 0;
        bindingDescription.stride = sizeof(Vertex);
        bindingDescription.inputRate = VK_VERTEX_INPUT_RATE_VERTEX;
        return bindingDescription;
    }

    static std::vector<VkVertexInputAttributeDescription> getAttributeDescriptions() {
        std::vector<VkVertexInputAttributeDescription> attributeDescriptions(5);

        attributeDescriptions[0].binding = 0;
        attributeDescriptions[0].location = 0;
        attributeDescriptions[0].format = VK_FORMAT_R32G32B32_SFLOAT;
        attributeDescriptions[0].offset = offsetof(Vertex, position);

        attributeDescriptions[1].binding = 0;
        attributeDescriptions[1].location = 1;
        attributeDescriptions[1].format = VK_FORMAT_R32G32B32_SFLOAT;
        attributeDescriptions[1].offset = offsetof(Vertex, normal);

        attributeDescriptions[2].binding = 0;
        attributeDescriptions[2].location = 2;
        attributeDescriptions[2].format = VK_FORMAT_R32G32_SFLOAT;
        attributeDescriptions[2].offset = offsetof(Vertex, texCoord);

        attributeDescriptions[3].binding = 0;
        attributeDescriptions[3].location = 3;
        attributeDescriptions[3].format = VK_FORMAT_R32G32B32_SFLOAT;
        attributeDescriptions[3].offset = offsetof(Vertex, tangent);

        attributeDescriptions[4].binding = 0;
        attributeDescriptions[4].location = 4;
        attributeDescriptions[4].format = VK_FORMAT_R32G32B32_SFLOAT;
        attributeDescriptions[4].offset = offsetof(Vertex, bitangent);

        return attributeDescriptions;
    }
};

struct RayHit {
    bool hit = false;
    float distance = 1e9f;
    glm::vec3 point{0.0f};
    glm::vec2 uv{0.0f};
    glm::vec3 normal{0.0f};
};

class Mesh {
public:
    std::vector<Vertex> vertices;
    std::vector<uint32_t> indices;

    VkBuffer vertexBuffer = VK_NULL_HANDLE;
    VkDeviceMemory vertexBufferMemory = VK_NULL_HANDLE;
    VkBuffer indexBuffer = VK_NULL_HANDLE;
    VkDeviceMemory indexBufferMemory = VK_NULL_HANDLE;

    std::string name = "Default Mesh";

    ~Mesh();

    void uploadToGPU(VkDevice device, VkPhysicalDevice physicalDevice, VkCommandPool commandPool, VkQueue queue);
    void destroyGPUResources(VkDevice device);

    bool intersectRay(const glm::vec3& rayOrigin, const glm::vec3& rayDir, const glm::mat4& modelMatrix, RayHit& outHit) const;

    static void computeTangents(std::vector<Vertex>& vertices, const std::vector<uint32_t>& indices);
    static Mesh createSphere(float radius = 1.0f, int rings = 48, int sectors = 48);
    static Mesh createCube(float size = 1.5f);
    static Mesh createTorus(float r1 = 0.9f, float r2 = 0.35f, int segments = 48, int sides = 24);
    static Mesh loadOBJ(const std::string& filepath);
};

} // namespace REMoAINTER
