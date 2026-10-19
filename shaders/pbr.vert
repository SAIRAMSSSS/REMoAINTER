#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in vec3 inTangent;
layout(location = 4) in vec3 inBitangent;

layout(location = 0) out vec3 outWorldPos;
layout(location = 1) out vec3 outNormal;
layout(location = 2) out vec2 outTexCoord;
layout(location = 3) out mat3 outTBN;

layout(push_constant) uniform PushConstants {
    mat4 model;
    mat4 viewProj;
    vec3 cameraPos;
    float lightIntensity;
} ubo;

void main() {
    vec4 worldPos = ubo.model * vec4(inPosition, 1.0);
    outWorldPos = worldPos.xyz;
    outTexCoord = inTexCoord;

    mat3 normalMatrix = transpose(inverse(mat3(ubo.model)));
    vec3 T = normalize(normalMatrix * inTangent);
    vec3 B = normalize(normalMatrix * inBitangent);
    vec3 N = normalize(normalMatrix * inNormal);
    outNormal = N;
    outTBN = mat3(T, B, N);

    gl_Position = ubo.viewProj * worldPos;
}
