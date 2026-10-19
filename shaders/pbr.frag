#version 450

layout(location = 0) in vec3 inWorldPos;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in mat3 inTBN;

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D baseColorMap;
layout(set = 0, binding = 1) uniform sampler2D metallicRoughnessMap; // G = Roughness, B = Metallic
layout(set = 0, binding = 2) uniform sampler2D normalMap;

layout(push_constant) uniform PushConstants {
    mat4 model;
    mat4 viewProj;
    vec3 cameraPos;
    float lightIntensity;
} ubo;

const float PI = 3.14159265359;

// GGX / Trowbridge-Reitz Normal Distribution Function
float distributionGGX(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;

    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom + 0.0000001);
}

// Schlick-GGX Geometry shadowing function
float geometrySchlickGGX(float NdotV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

// Smith's method for geometry term
float geometrySmith(vec3 N, vec3 V, vec3 L, float roughness) {
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = geometrySchlickGGX(NdotV, roughness);
    float ggx1 = geometrySchlickGGX(NdotL, roughness);
    return ggx1 * ggx2;
}

// Fresnel-Schlick approximation
vec3 fresnelSchlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

void main() {
    vec4 baseColorSample = texture(baseColorMap, inTexCoord);
    vec3 albedo = pow(baseColorSample.rgb, vec3(2.2)); // Convert sRGB to Linear
    float alpha = baseColorSample.a;

    vec4 mrSample = texture(metallicRoughnessMap, inTexCoord);
    float roughness = clamp(mrSample.g, 0.04, 1.0);
    float metallic  = clamp(mrSample.b, 0.0, 1.0);

    // Sample and unpack normal
    vec3 normalSample = texture(normalMap, inTexCoord).rgb;
    normalSample = normalize(normalSample * 2.0 - 1.0);
    vec3 N = normalize(inTBN * normalSample);

    vec3 V = normalize(ubo.cameraPos - inWorldPos);
    vec3 R = reflect(-V, N);

    vec3 F0 = vec3(0.04);
    F0 = mix(F0, albedo, metallic);

    // Directional Key Light & Fill Light
    vec3 lightDirs[2] = vec3[](
        normalize(vec3(0.7, 1.0, 0.5)),
        normalize(vec3(-0.8, 0.3, -0.6))
    );
    vec3 lightColors[2] = vec3[](
        vec3(3.0, 2.9, 2.8) * ubo.lightIntensity,
        vec3(0.8, 0.9, 1.1) * (ubo.lightIntensity * 0.4)
    );

    vec3 Lo = vec3(0.0);
    for (int i = 0; i < 2; ++i) {
        vec3 L = lightDirs[i];
        vec3 H = normalize(V + L);
        float NdotL = max(dot(N, L), 0.0);

        if (NdotL > 0.0) {
            float NDF = distributionGGX(N, H, roughness);
            float G = geometrySmith(N, V, L, roughness);
            vec3 F = fresnelSchlick(max(dot(H, V), 0.0), F0);

            vec3 numerator = NDF * G * F;
            float denominator = 4.0 * max(dot(N, V), 0.0) * NdotL + 0.0001;
            vec3 specular = numerator / denominator;

            vec3 kS = F;
            vec3 kD = vec3(1.0) - kS;
            kD *= 1.0 - metallic;

            Lo += (kD * albedo / PI + specular) * lightColors[i] * NdotL;
        }
    }

    // Ambient / Environment Approximation
    vec3 ambient = vec3(0.03) * albedo;
    vec3 color = ambient + Lo;

    // HDR Reinhard tone mapping & gamma correction
    color = color / (color + vec3(1.0));
    color = pow(color, vec3(1.0 / 2.2)); // Convert back to sRGB

    outColor = vec4(color, alpha);
}
