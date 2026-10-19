#include "Camera.hpp"
#include <algorithm>

namespace REMoAINTER {

void Camera::orbit(float deltaYaw, float deltaPitch) {
    yaw += deltaYaw;
    pitch += deltaPitch;
    pitch = std::clamp(pitch, minPitch, maxPitch);
}

void Camera::pan(float deltaX, float deltaY) {
    glm::vec3 pos = getPosition();
    glm::vec3 forward = glm::normalize(target - pos);
    glm::vec3 right = glm::normalize(glm::cross(forward, glm::vec3(0.0f, 1.0f, 0.0f)));
    glm::vec3 up = glm::normalize(glm::cross(right, forward));

    float panSpeed = distance * 0.0015f;
    target += (-right * deltaX + up * deltaY) * panSpeed;
}

void Camera::zoom(float deltaDistance) {
    distance -= deltaDistance * (distance * 0.1f);
    distance = std::clamp(distance, minDistance, maxDistance);
}

glm::vec3 Camera::getPosition() const {
    float radYaw = glm::radians(yaw);
    float radPitch = glm::radians(pitch);

    float x = distance * std::cos(radPitch) * std::sin(radYaw);
    float y = distance * std::sin(radPitch);
    float z = distance * std::cos(radPitch) * std::cos(radYaw);

    return target + glm::vec3(x, y, z);
}

glm::mat4 Camera::getViewMatrix() const {
    return glm::lookAt(getPosition(), target, glm::vec3(0.0f, 1.0f, 0.0f));
}

glm::mat4 Camera::getProjectionMatrix(float aspect, float nearPlane, float farPlane) const {
    glm::mat4 proj = glm::perspective(glm::radians(fov), aspect, nearPlane, farPlane);
    // Vulkan NDC has Y pointing downwards, unlike OpenGL
    proj[1][1] *= -1.0f;
    return proj;
}

void Camera::screenPointToRay(float mouseX, float mouseY, float screenWidth, float screenHeight,
                              glm::vec3& outOrigin, glm::vec3& outDir) const {
    float ndcX = (2.0f * mouseX) / screenWidth - 1.0f;
    float ndcY = (2.0f * mouseY) / screenHeight - 1.0f;

    // Invert Vulkan Y adjustment for screen to world ray
    glm::mat4 proj = getProjectionMatrix(screenWidth / screenHeight);
    glm::mat4 view = getViewMatrix();
    glm::mat4 invVP = glm::inverse(proj * view);

    glm::vec4 nearPointNDC(ndcX, ndcY, 0.05f, 1.0f);
    glm::vec4 farPointNDC(ndcX, ndcY, 0.99f, 1.0f);

    glm::vec4 nearWorld = invVP * nearPointNDC;
    glm::vec4 farWorld = invVP * farPointNDC;

    nearWorld /= nearWorld.w;
    farWorld /= farWorld.w;

    outOrigin = getPosition();
    outDir = glm::normalize(glm::vec3(farWorld - nearWorld));
}

} // namespace REMoAINTER
