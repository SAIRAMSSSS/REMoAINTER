#pragma once

#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>

namespace REMoAINTER {

class Camera {
public:
    glm::vec3 target{0.0f, 0.0f, 0.0f};
    float distance = 3.5f;
    float yaw = 45.0f;
    float pitch = 25.0f;
    float fov = 45.0f;

    float minDistance = 0.5f;
    float maxDistance = 50.0f;
    float minPitch = -89.0f;
    float maxPitch = 89.0f;

    void orbit(float deltaYaw, float deltaPitch);
    void pan(float deltaX, float deltaY);
    void zoom(float deltaDistance);
    void resetToFocus(float dist = 3.5f);

    glm::vec3 getPosition() const;
    glm::mat4 getViewMatrix() const;
    glm::mat4 getProjectionMatrix(float aspect, float nearPlane = 0.05f, float farPlane = 100.0f) const;

    void screenPointToRay(float mouseX, float mouseY, float screenWidth, float screenHeight,
                          glm::vec3& outOrigin, glm::vec3& outDir) const;
};

} // namespace REMoAINTER
