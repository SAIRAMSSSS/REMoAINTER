#include "Mesh.hpp"
#include <cmath>
#include <iostream>
#include <cstring>
#include <stdexcept>

#define TINYOBJLOADER_IMPLEMENTATION
#include "../../third_party/tinyobjloader/tiny_obj_loader.h"

namespace REMoAINTER {

static uint32_t findMemoryType(VkPhysicalDevice physicalDevice, uint32_t typeFilter, VkMemoryPropertyFlags properties) {
    VkPhysicalDeviceMemoryProperties memProperties;
    vkGetPhysicalDeviceMemoryProperties(physicalDevice, &memProperties);

    for (uint32_t i = 0; i < memProperties.memoryTypeCount; i++) {
        if ((typeFilter & (1 << i)) && (memProperties.memoryTypes[i].propertyFlags & properties) == properties) {
            return i;
        }
    }
    throw std::runtime_error("failed to find suitable memory type!");
}

static void createBuffer(VkDevice device, VkPhysicalDevice physicalDevice, VkDeviceSize size, VkBufferUsageFlags usage,
                         VkMemoryPropertyFlags properties, VkBuffer& buffer, VkDeviceMemory& bufferMemory) {
    VkBufferCreateInfo bufferInfo{};
    bufferInfo.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
    bufferInfo.size = size;
    bufferInfo.usage = usage;
    bufferInfo.sharingMode = VK_SHARING_MODE_EXCLUSIVE;

    if (vkCreateBuffer(device, &bufferInfo, nullptr, &buffer) != VK_SUCCESS) {
        throw std::runtime_error("failed to create buffer!");
    }

    VkMemoryRequirements memRequirements;
    vkGetBufferMemoryRequirements(device, buffer, &memRequirements);

    VkMemoryAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    allocInfo.allocationSize = memRequirements.size;
    allocInfo.memoryTypeIndex = findMemoryType(physicalDevice, memRequirements.memoryTypeBits, properties);

    if (vkAllocateMemory(device, &allocInfo, nullptr, &bufferMemory) != VK_SUCCESS) {
        throw std::runtime_error("failed to allocate buffer memory!");
    }

    vkBindBufferMemory(device, buffer, bufferMemory, 0);
}

static void copyBuffer(VkDevice device, VkCommandPool commandPool, VkQueue queue, VkBuffer srcBuffer, VkBuffer dstBuffer, VkDeviceSize size) {
    VkCommandBufferAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
    allocInfo.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    allocInfo.commandPool = commandPool;
    allocInfo.commandBufferCount = 1;

    VkCommandBuffer commandBuffer;
    vkAllocateCommandBuffers(device, &allocInfo, &commandBuffer);

    VkCommandBufferBeginInfo beginInfo{};
    beginInfo.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
    beginInfo.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;

    vkBeginCommandBuffer(commandBuffer, &beginInfo);
    VkBufferCopy copyRegion{};
    copyRegion.size = size;
    vkCmdCopyBuffer(commandBuffer, srcBuffer, dstBuffer, 1, &copyRegion);
    vkEndCommandBuffer(commandBuffer);

    VkSubmitInfo submitInfo{};
    submitInfo.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO;
    submitInfo.commandBufferCount = 1;
    submitInfo.pCommandBuffers = &commandBuffer;

    vkQueueSubmit(queue, 1, &submitInfo, VK_NULL_HANDLE);
    vkQueueWaitIdle(queue);

    vkFreeCommandBuffers(device, commandPool, 1, &commandBuffer);
}

Mesh::~Mesh() {
}

void Mesh::destroyGPUResources(VkDevice device) {
    if (vertexBuffer != VK_NULL_HANDLE) {
        vkDestroyBuffer(device, vertexBuffer, nullptr);
        vkFreeMemory(device, vertexBufferMemory, nullptr);
        vertexBuffer = VK_NULL_HANDLE;
        vertexBufferMemory = VK_NULL_HANDLE;
    }
    if (indexBuffer != VK_NULL_HANDLE) {
        vkDestroyBuffer(device, indexBuffer, nullptr);
        vkFreeMemory(device, indexBufferMemory, nullptr);
        indexBuffer = VK_NULL_HANDLE;
        indexBufferMemory = VK_NULL_HANDLE;
    }
}

void Mesh::uploadToGPU(VkDevice device, VkPhysicalDevice physicalDevice, VkCommandPool commandPool, VkQueue queue) {
    destroyGPUResources(device);

    VkDeviceSize vertexSize = sizeof(Vertex) * vertices.size();
    VkDeviceSize indexSize = sizeof(uint32_t) * indices.size();

    // Vertex staging buffer
    VkBuffer stagingVertexBuffer;
    VkDeviceMemory stagingVertexBufferMemory;
    createBuffer(device, physicalDevice, vertexSize, VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                 VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                 stagingVertexBuffer, stagingVertexBufferMemory);

    void* data;
    vkMapMemory(device, stagingVertexBufferMemory, 0, vertexSize, 0, &data);
    memcpy(data, vertices.data(), (size_t)vertexSize);
    vkUnmapMemory(device, stagingVertexBufferMemory);

    createBuffer(device, physicalDevice, vertexSize, VK_BUFFER_USAGE_TRANSFER_DST_BIT | VK_BUFFER_USAGE_VERTEX_BUFFER_BIT,
                 VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, vertexBuffer, vertexBufferMemory);

    copyBuffer(device, commandPool, queue, stagingVertexBuffer, vertexBuffer, vertexSize);
    vkDestroyBuffer(device, stagingVertexBuffer, nullptr);
    vkFreeMemory(device, stagingVertexBufferMemory, nullptr);

    // Index staging buffer
    VkBuffer stagingIndexBuffer;
    VkDeviceMemory stagingIndexBufferMemory;
    createBuffer(device, physicalDevice, indexSize, VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                 VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                 stagingIndexBuffer, stagingIndexBufferMemory);

    vkMapMemory(device, stagingIndexBufferMemory, 0, indexSize, 0, &data);
    memcpy(data, indices.data(), (size_t)indexSize);
    vkUnmapMemory(device, stagingIndexBufferMemory);

    createBuffer(device, physicalDevice, indexSize, VK_BUFFER_USAGE_TRANSFER_DST_BIT | VK_BUFFER_USAGE_INDEX_BUFFER_BIT,
                 VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, indexBuffer, indexBufferMemory);

    copyBuffer(device, commandPool, queue, stagingIndexBuffer, indexBuffer, indexSize);
    vkDestroyBuffer(device, stagingIndexBuffer, nullptr);
    vkFreeMemory(device, stagingIndexBufferMemory, nullptr);
}

void Mesh::computeTangents(std::vector<Vertex>& vertices, const std::vector<uint32_t>& indices) {
    for (auto& v : vertices) {
        v.tangent = glm::vec3(0.0f);
        v.bitangent = glm::vec3(0.0f);
    }

    for (size_t i = 0; i < indices.size(); i += 3) {
        uint32_t i0 = indices[i];
        uint32_t i1 = indices[i + 1];
        uint32_t i2 = indices[i + 2];

        const Vertex& v0 = vertices[i0];
        const Vertex& v1 = vertices[i1];
        const Vertex& v2 = vertices[i2];

        glm::vec3 edge1 = v1.position - v0.position;
        glm::vec3 edge2 = v2.position - v0.position;

        glm::vec2 deltaUV1 = v1.texCoord - v0.texCoord;
        glm::vec2 deltaUV2 = v2.texCoord - v0.texCoord;

        float f = 1.0f / (deltaUV1.x * deltaUV2.y - deltaUV2.x * deltaUV1.y + 0.000001f);

        glm::vec3 tangent = f * (deltaUV2.y * edge1 - deltaUV1.y * edge2);
        glm::vec3 bitangent = f * (-deltaUV2.x * edge1 + deltaUV1.x * edge2);

        vertices[i0].tangent += tangent;
        vertices[i1].tangent += tangent;
        vertices[i2].tangent += tangent;

        vertices[i0].bitangent += bitangent;
        vertices[i1].bitangent += bitangent;
        vertices[i2].bitangent += bitangent;
    }

    for (auto& v : vertices) {
        if (glm::length(v.tangent) > 0.0001f) {
            v.tangent = glm::normalize(v.tangent - v.normal * glm::dot(v.normal, v.tangent));
        } else {
            v.tangent = glm::vec3(1.0f, 0.0f, 0.0f);
        }
        if (glm::length(v.bitangent) > 0.0001f) {
            v.bitangent = glm::normalize(v.bitangent);
        } else {
            v.bitangent = glm::cross(v.normal, v.tangent);
        }
    }
}

bool Mesh::intersectRay(const glm::vec3& rayOrigin, const glm::vec3& rayDir, const glm::mat4& modelMatrix, RayHit& outHit) const {
    glm::mat4 invModel = glm::inverse(modelMatrix);
    glm::vec3 localOrigin = glm::vec3(invModel * glm::vec4(rayOrigin, 1.0f));
    glm::vec3 localDir = glm::normalize(glm::vec3(invModel * glm::vec4(rayDir, 0.0f)));

    bool found = false;
    float closestDist = 1e9f;

    for (size_t i = 0; i < indices.size(); i += 3) {
        const Vertex& v0 = vertices[indices[i]];
        const Vertex& v1 = vertices[indices[i + 1]];
        const Vertex& v2 = vertices[indices[i + 2]];

        glm::vec3 e1 = v1.position - v0.position;
        glm::vec3 e2 = v2.position - v0.position;
        glm::vec3 pvec = glm::cross(localDir, e2);
        float det = glm::dot(e1, pvec);

        if (std::abs(det) < 1e-7f) continue;
        float invDet = 1.0f / det;

        glm::vec3 tvec = localOrigin - v0.position;
        float u = glm::dot(tvec, pvec) * invDet;
        if (u < 0.0f || u > 1.0f) continue;

        glm::vec3 qvec = glm::cross(tvec, e1);
        float v = glm::dot(localDir, qvec) * invDet;
        if (v < 0.0f || u + v > 1.0f) continue;

        float t = glm::dot(e2, qvec) * invDet;
        if (t > 1e-4f && t < closestDist) {
            closestDist = t;
            found = true;

            float w = 1.0f - u - v;
            glm::vec2 interpolatedUV = w * v0.texCoord + u * v1.texCoord + v * v2.texCoord;
            glm::vec3 localHitPoint = localOrigin + localDir * t;
            glm::vec3 localNormal = glm::normalize(w * v0.normal + u * v1.normal + v * v2.normal);

            outHit.hit = true;
            outHit.distance = closestDist;
            outHit.point = glm::vec3(modelMatrix * glm::vec4(localHitPoint, 1.0f));
            outHit.normal = glm::normalize(glm::mat3(modelMatrix) * localNormal);
            outHit.uv = interpolatedUV;
        }
    }

    return found;
}

Mesh Mesh::createSphere(float radius, int rings, int sectors) {
    Mesh mesh;
    mesh.name = "PBR Material Sphere";
    const float R = 1.0f / (float)(rings - 1);
    const float S = 1.0f / (float)(sectors - 1);
    const float PI = 3.14159265358979323846f;

    for (int r = 0; r < rings; ++r) {
        for (int s = 0; s < sectors; ++s) {
            float y = std::sin(-PI / 2.0f + PI * r * R);
            float x = std::cos(2.0f * PI * s * S) * std::sin(PI * r * R);
            float z = std::sin(2.0f * PI * s * S) * std::sin(PI * r * R);

            Vertex v;
            v.position = glm::vec3(x, y, z) * radius;
            v.normal = glm::normalize(v.position);
            v.texCoord = glm::vec2(s * S, r * R);
            mesh.vertices.push_back(v);
        }
    }

    for (int r = 0; r < rings - 1; ++r) {
        for (int s = 0; s < sectors - 1; ++s) {
            uint32_t i0 = r * sectors + s;
            uint32_t i1 = (r + 1) * sectors + s;
            uint32_t i2 = (r + 1) * sectors + (s + 1);
            uint32_t i3 = r * sectors + (s + 1);

            mesh.indices.push_back(i0);
            mesh.indices.push_back(i1);
            mesh.indices.push_back(i2);

            mesh.indices.push_back(i0);
            mesh.indices.push_back(i2);
            mesh.indices.push_back(i3);
        }
    }

    computeTangents(mesh.vertices, mesh.indices);
    return mesh;
}

Mesh Mesh::createCube(float size) {
    Mesh mesh;
    mesh.name = "PBR Test Cube";
    float h = size * 0.5f;

    struct Face {
        glm::vec3 n;
        glm::vec3 p[4];
    };

    Face faces[6] = {
        // Front (+Z)
        { {0, 0, 1}, { {-h, -h,  h}, { h, -h,  h}, { h,  h,  h}, {-h,  h,  h} } },
        // Back (-Z)
        { {0, 0, -1}, { { h, -h, -h}, {-h, -h, -h}, {-h,  h, -h}, { h,  h, -h} } },
        // Top (+Y)
        { {0, 1, 0}, { {-h,  h,  h}, { h,  h,  h}, { h,  h, -h}, {-h,  h, -h} } },
        // Bottom (-Y)
        { {0, -1, 0}, { {-h, -h, -h}, { h, -h, -h}, { h, -h,  h}, {-h, -h,  h} } },
        // Right (+X)
        { {1, 0, 0}, { { h, -h,  h}, { h, -h, -h}, { h,  h, -h}, { h,  h,  h} } },
        // Left (-X)
        { {-1, 0, 0}, { {-h, -h, -h}, {-h, -h,  h}, {-h,  h,  h}, {-h,  h, -h} } }
    };

    glm::vec2 uvs[4] = { {0, 0}, {1, 0}, {1, 1}, {0, 1} };

    for (int f = 0; f < 6; ++f) {
        uint32_t baseIdx = (uint32_t)mesh.vertices.size();
        for (int v = 0; v < 4; ++v) {
            Vertex vert;
            vert.position = faces[f].p[v];
            vert.normal = faces[f].n;
            vert.texCoord = uvs[v];
            mesh.vertices.push_back(vert);
        }
        mesh.indices.push_back(baseIdx + 0);
        mesh.indices.push_back(baseIdx + 1);
        mesh.indices.push_back(baseIdx + 2);

        mesh.indices.push_back(baseIdx + 0);
        mesh.indices.push_back(baseIdx + 2);
        mesh.indices.push_back(baseIdx + 3);
    }

    computeTangents(mesh.vertices, mesh.indices);
    return mesh;
}

Mesh Mesh::createTorus(float r1, float r2, int segments, int sides) {
    Mesh mesh;
    mesh.name = "PBR Material Torus";
    const float PI = 3.14159265358979323846f;

    for (int i = 0; i <= segments; ++i) {
        float u = (float)i / (float)segments;
        float theta = u * 2.0f * PI;

        for (int j = 0; j <= sides; ++j) {
            float v = (float)j / (float)sides;
            float phi = v * 2.0f * PI;

            float x = (r1 + r2 * std::cos(phi)) * std::cos(theta);
            float y = r2 * std::sin(phi);
            float z = (r1 + r2 * std::cos(phi)) * std::sin(theta);

            glm::vec3 center(r1 * std::cos(theta), 0.0f, r1 * std::sin(theta));
            glm::vec3 pos(x, y, z);

            Vertex vert;
            vert.position = pos;
            vert.normal = glm::normalize(pos - center);
            vert.texCoord = glm::vec2(u * 2.0f, v);
            mesh.vertices.push_back(vert);
        }
    }

    for (int i = 0; i < segments; ++i) {
        for (int j = 0; j < sides; ++j) {
            uint32_t first = i * (sides + 1) + j;
            uint32_t second = (i + 1) * (sides + 1) + j;

            mesh.indices.push_back(first);
            mesh.indices.push_back(second);
            mesh.indices.push_back(first + 1);

            mesh.indices.push_back(second);
            mesh.indices.push_back(second + 1);
            mesh.indices.push_back(first + 1);
        }
    }

    computeTangents(mesh.vertices, mesh.indices);
    return mesh;
}

Mesh Mesh::loadOBJ(const std::string& filepath) {
    Mesh mesh;
    mesh.name = filepath;

    tinyobj::attrib_t attrib;
    std::vector<tinyobj::shape_t> shapes;
    std::vector<tinyobj::material_t> materials;
    std::string warn, err;

    if (!tinyobj::LoadObj(&attrib, &shapes, &materials, &warn, &err, filepath.c_str())) {
        std::cerr << "Failed to load OBJ: " << warn << " " << err << std::endl;
        return createSphere();
    }

    for (const auto& shape : shapes) {
        for (const auto& index : shape.mesh.indices) {
            Vertex vertex{};
            vertex.position = {
                attrib.vertices[3 * index.vertex_index + 0],
                attrib.vertices[3 * index.vertex_index + 1],
                attrib.vertices[3 * index.vertex_index + 2]
            };

            if (index.normal_index >= 0) {
                vertex.normal = {
                    attrib.normals[3 * index.normal_index + 0],
                    attrib.normals[3 * index.normal_index + 1],
                    attrib.normals[3 * index.normal_index + 2]
                };
            } else {
                vertex.normal = glm::vec3(0.0f, 1.0f, 0.0f);
            }

            if (index.texcoord_index >= 0) {
                vertex.texCoord = {
                    attrib.texcoords[2 * index.texcoord_index + 0],
                    1.0f - attrib.texcoords[2 * index.texcoord_index + 1]
                };
            } else {
                vertex.texCoord = glm::vec2(0.0f);
            }

            mesh.vertices.push_back(vertex);
            mesh.indices.push_back((uint32_t)mesh.indices.size());
        }
    }

    computeTangents(mesh.vertices, mesh.indices);
    return mesh;
}

} // namespace REMoAINTER
