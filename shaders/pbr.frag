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
    float envRotation;     // Shift+RMB rotates lighting
    int displayChannel;    // 0: Lit, 1: BaseColor, 2: Roughness, 3: Metallic, 4: Normal, 5: LightingOnly
    int envPreset;         // 0: Studio, 1: Sunset, 2: Cyberpunk, 3: Forest, 4: Desert
} ubo;

const float PI = 3.14159265359;

float distributionGGX(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom + 0.0000001);
}

float geometrySchlickGGX(float NdotV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

float geometrySmith(vec3 N, vec3 V, vec3 L, float roughness) {
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = geometrySchlickGGX(NdotV, roughness);
    float ggx1 = geometrySchlickGGX(NdotL, roughness);
    return ggx1 * ggx2;
}

vec3 fresnelSchlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

vec3 fresnelSchlickRoughness(float cosTheta, vec3 F0, float roughness) {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// Procedural Environment Lighting Model
void getEnvironmentLighting(int preset, float rotation, vec3 N, vec3 V, float roughness, out vec3 ambientDiffuse, out vec3 ambientSpecular, out vec3 keyDir, out vec3 keyColor, out vec3 fillDir, out vec3 fillColor) {
    // Rotate directional vectors around Y axis
    float c = cos(rotation);
    float s = sin(rotation);
    mat3 rotY = mat3(
        c, 0.0, s,
        0.0, 1.0, 0.0,
        -s, 0.0, c
    );

    vec3 R = reflect(-V, N);

    vec3 zenithColor;
    vec3 horizonColor;
    vec3 groundColor;

    if (preset == 1) {
        // Sunset Warm
        zenithColor   = vec3(0.25, 0.20, 0.45);
        horizonColor  = vec3(1.00, 0.45, 0.15);
        groundColor   = vec3(0.12, 0.08, 0.05);
        keyDir        = rotY * normalize(vec3(0.9, 0.35, 0.4));
        keyColor      = vec3(3.5, 1.6, 0.6);
        fillDir       = rotY * normalize(vec3(-0.8, 0.5, -0.6));
        fillColor     = vec3(0.4, 0.5, 0.9);
    } else if (preset == 2) {
        // Cyberpunk Neon
        zenithColor   = vec3(0.08, 0.02, 0.18);
        horizonColor  = vec3(0.85, 0.05, 0.55);
        groundColor   = vec3(0.02, 0.10, 0.15);
        keyDir        = rotY * normalize(vec3(0.7, 0.8, 0.3));
        keyColor      = vec3(0.2, 2.5, 3.2); // Cyan Key
        fillDir       = rotY * normalize(vec3(-0.7, 0.3, -0.7));
        fillColor     = vec3(3.2, 0.3, 2.0); // Magenta Fill
    } else if (preset == 3) {
        // Forest Sunlight
        zenithColor   = vec3(0.20, 0.40, 0.65);
        horizonColor  = vec3(0.55, 0.70, 0.40);
        groundColor   = vec3(0.15, 0.22, 0.08);
        keyDir        = rotY * normalize(vec3(0.5, 1.0, 0.5));
        keyColor      = vec3(3.2, 3.1, 2.6);
        fillDir       = rotY * normalize(vec3(-0.6, 0.3, -0.6));
        fillColor     = vec3(0.4, 0.7, 0.5);
    } else if (preset == 4) {
        // Desert High Noon
        zenithColor   = vec3(0.35, 0.55, 0.85);
        horizonColor  = vec3(0.85, 0.75, 0.60);
        groundColor   = vec3(0.45, 0.35, 0.20);
        keyDir        = rotY * normalize(vec3(0.2, 1.0, 0.1));
        keyColor      = vec3(4.5, 4.2, 3.8);
        fillDir       = rotY * normalize(vec3(-0.5, 0.2, -0.8));
        fillColor     = vec3(0.8, 0.7, 0.5);
    } else {
        // Studio Neutral (Default)
        zenithColor   = vec3(0.35, 0.38, 0.42);
        horizonColor  = vec3(0.50, 0.52, 0.55);
        groundColor   = vec3(0.18, 0.18, 0.20);
        keyDir        = rotY * normalize(vec3(0.65, 0.85, 0.55));
        keyColor      = vec3(2.8, 2.8, 2.8);
        fillDir       = rotY * normalize(vec3(-0.7, 0.4, -0.6));
        fillColor     = vec3(1.1, 1.2, 1.3);
    }

    // Hemisphere diffuse irradiance
    float upFraction = N.y * 0.5 + 0.5;
    ambientDiffuse = mix(groundColor, mix(horizonColor, zenithColor, max(N.y, 0.0)), upFraction) * 0.45;

    // Specular environment reflection approximation
    float specUp = R.y * 0.5 + 0.5;
    vec3 skyReflect = mix(groundColor, mix(horizonColor, zenithColor, max(R.y, 0.0)), specUp);
    ambientSpecular = mix(skyReflect, ambientDiffuse, roughness * 0.75);
}

void main() {
    vec4 baseColorSample = texture(baseColorMap, inTexCoord);
    vec3 albedo = pow(baseColorSample.rgb, vec3(2.2)); // Convert sRGB to Linear
    float alpha = baseColorSample.a;

    vec4 mrSample = texture(metallicRoughnessMap, inTexCoord);
    float roughness = clamp(mrSample.g, 0.04, 1.0);
    float metallic  = clamp(mrSample.b, 0.0, 1.0);

    // Unpack normal map
    vec3 normalSample = texture(normalMap, inTexCoord).rgb;
    normalSample = normalize(normalSample * 2.0 - 1.0);
    vec3 N = normalize(inTBN * normalSample);

    // ==========================================
    // Display Mode Solo Channel Overrides
    // ==========================================
    if (ubo.displayChannel == 1) {
        // Base Color Solo
        outColor = vec4(pow(albedo, vec3(1.0 / 2.2)), 1.0);
        return;
    } else if (ubo.displayChannel == 2) {
        // Roughness Solo (Grayscale)
        outColor = vec4(vec3(roughness), 1.0);
        return;
    } else if (ubo.displayChannel == 3) {
        // Metallic Solo (Grayscale)
        outColor = vec4(vec3(metallic), 1.0);
        return;
    } else if (ubo.displayChannel == 4) {
        // Normal Map Solo (Tangent Space RGB)
        outColor = vec4(normalSample * 0.5 + 0.5, 1.0);
        return;
    } else if (ubo.displayChannel == 5) {
        // Lighting Only (Neutral Clay Material)
        albedo = vec3(0.75);
        metallic = 0.0;
        roughness = 0.4;
    }

    vec3 V = normalize(ubo.cameraPos - inWorldPos);
    vec3 F0 = mix(vec3(0.04), albedo, metallic);

    // Setup Environment Lighting
    vec3 ambientDiffuse, ambientSpecular;
    vec3 keyDir, keyColor, fillDir, fillColor;
    getEnvironmentLighting(ubo.envPreset, ubo.envRotation, N, V, roughness,
                           ambientDiffuse, ambientSpecular, keyDir, keyColor, fillDir, fillColor);

    keyColor *= ubo.lightIntensity;
    fillColor *= ubo.lightIntensity;

    // Direct Lighting Evaluation (Cook-Torrance GGX)
    vec3 lightDirs[2]   = vec3[](keyDir, fillDir);
    vec3 lightColors[2] = vec3[](keyColor, fillColor);

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
            vec3 kD = (vec3(1.0) - kS) * (1.0 - metallic);

            Lo += (kD * albedo / PI + specular) * lightColors[i] * NdotL;
        }
    }

    // Indirect / IBL Approximation
    vec3 kS = fresnelSchlickRoughness(max(dot(N, V), 0.0), F0, roughness);
    vec3 kD = (1.0 - kS) * (1.0 - metallic);
    vec3 diffuseIBL = kD * ambientDiffuse * albedo;
    vec3 specularIBL = ambientSpecular * kS;
    vec3 ambient = (diffuseIBL + specularIBL) * ubo.lightIntensity;

    vec3 color = ambient + Lo;

    // ACES Film Tone Mapping Curve for rich cinematic colors
    vec3 a = color * (color + 0.0245786) - 0.000090537;
    vec3 b = color * (0.983729 * color + 0.4329510) + 0.238081;
    color = clamp(a / b, 0.0, 1.0);

    // Convert back to sRGB display space
    color = pow(color, vec3(1.0 / 2.2));

    outColor = vec4(color, alpha);
}
