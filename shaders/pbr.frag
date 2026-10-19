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
    int envPreset;         // 0: Studio Neutral, 1: Warm & Cool Studio, 2: Cyberpunk, 3: Forest, 4: Desert
} ubo;

const float PI = 3.14159265359;

// ==========================================
// Cook-Torrance GGX Microfacet BRDF
// ==========================================
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

// ==========================================
// 5-Light Studio Rig & Environment
// ==========================================
struct StudioRig {
    vec3 lightDirs[5];
    vec3 lightColors[5];
    vec3 ambientSky;
    vec3 ambientGround;
    vec3 softboxColor1;
    vec3 softboxDir1;
    vec3 softboxColor2;
    vec3 softboxDir2;
};

StudioRig getStudioRig(int preset, float rotation) {
    StudioRig rig;

    float c = cos(rotation);
    float s = sin(rotation);
    mat3 rotY = mat3(
        c, 0.0, s,
        0.0, 1.0, 0.0,
        -s, 0.0, c
    );

    if (preset == 1) {
        // Warm & Cool Studio (Substance Painter Key/Fill setup matching reference)
        rig.lightDirs[0] = rotY * normalize(vec3(0.75, 0.85, 0.60)); // Main Key
        rig.lightColors[0] = vec3(3.2, 2.7, 2.2);                     // Warm 4800K

        rig.lightDirs[1] = rotY * normalize(vec3(-0.85, 0.40, -0.40)); // Cool Fill
        rig.lightColors[1] = vec3(1.1, 1.4, 1.9);                      // Cool 7000K

        rig.lightDirs[2] = rotY * normalize(vec3(-0.30, 0.70, -0.90)); // Sharp Back Rim
        rig.lightColors[2] = vec3(2.4, 2.5, 2.6);                      // Crisp Rim

        rig.lightDirs[3] = rotY * normalize(vec3(0.0, 1.0, 0.15));      // Overhead Softbox
        rig.lightColors[3] = vec3(1.4, 1.35, 1.3);

        rig.lightDirs[4] = normalize(vec3(0.0, -0.9, 0.4));            // Table Bounce
        rig.lightColors[4] = vec3(0.8, 0.5, 0.3);                      // Warm Wood Bounce

        rig.ambientSky    = vec3(0.32, 0.34, 0.38);
        rig.ambientGround = vec3(0.18, 0.14, 0.11);

        rig.softboxDir1   = rig.lightDirs[0];
        rig.softboxColor1 = vec3(2.5, 2.2, 1.9);
        rig.softboxDir2   = rig.lightDirs[1];
        rig.softboxColor2 = vec3(1.2, 1.4, 1.8);
    } else if (preset == 2) {
        // Cyberpunk Neon
        rig.lightDirs[0] = rotY * normalize(vec3(0.8, 0.6, 0.4));
        rig.lightColors[0] = vec3(0.2, 3.2, 3.8); // Cyan Key

        rig.lightDirs[1] = rotY * normalize(vec3(-0.8, 0.5, -0.6));
        rig.lightColors[1] = vec3(3.8, 0.3, 2.4); // Magenta Fill

        rig.lightDirs[2] = rotY * normalize(vec3(0.1, 0.9, -0.9));
        rig.lightColors[2] = vec3(2.0, 0.8, 3.5); // Purple Rim

        rig.lightDirs[3] = rotY * normalize(vec3(0.0, 1.0, 0.0));
        rig.lightColors[3] = vec3(0.5, 1.5, 2.0);

        rig.lightDirs[4] = normalize(vec3(0.0, -1.0, 0.0));
        rig.lightColors[4] = vec3(0.2, 0.1, 0.3);

        rig.ambientSky    = vec3(0.10, 0.08, 0.20);
        rig.ambientGround = vec3(0.04, 0.06, 0.08);

        rig.softboxDir1   = rig.lightDirs[0];
        rig.softboxColor1 = vec3(0.3, 3.0, 3.5);
        rig.softboxDir2   = rig.lightDirs[1];
        rig.softboxColor2 = vec3(3.5, 0.4, 2.2);
    } else if (preset == 3) {
        // Forest Daylight
        rig.lightDirs[0] = rotY * normalize(vec3(0.5, 0.95, 0.4));
        rig.lightColors[0] = vec3(3.6, 3.4, 2.8); // Sun

        rig.lightDirs[1] = rotY * normalize(vec3(-0.7, 0.4, -0.6));
        rig.lightColors[1] = vec3(0.6, 0.9, 1.3); // Sky fill

        rig.lightDirs[2] = rotY * normalize(vec3(0.2, 0.3, -0.9));
        rig.lightColors[2] = vec3(1.5, 1.6, 1.4);

        rig.lightDirs[3] = rotY * normalize(vec3(0.0, 1.0, 0.0));
        rig.lightColors[3] = vec3(1.2, 1.4, 1.0);

        rig.lightDirs[4] = normalize(vec3(0.0, -1.0, 0.0));
        rig.lightColors[4] = vec3(0.3, 0.5, 0.2); // Foliage bounce

        rig.ambientSky    = vec3(0.25, 0.40, 0.60);
        rig.ambientGround = vec3(0.12, 0.20, 0.08);

        rig.softboxDir1   = rig.lightDirs[0];
        rig.softboxColor1 = vec3(3.0, 2.8, 2.2);
        rig.softboxDir2   = rig.lightDirs[1];
        rig.softboxColor2 = vec3(0.8, 1.1, 1.4);
    } else if (preset == 4) {
        // Desert High Noon
        rig.lightDirs[0] = rotY * normalize(vec3(0.2, 1.0, 0.15));
        rig.lightColors[0] = vec3(4.5, 4.2, 3.6); // Direct Noon Sun

        rig.lightDirs[1] = rotY * normalize(vec3(-0.6, 0.3, -0.7));
        rig.lightColors[1] = vec3(0.9, 1.1, 1.5);

        rig.lightDirs[2] = rotY * normalize(vec3(0.8, 0.2, -0.5));
        rig.lightColors[2] = vec3(1.6, 1.4, 1.2);

        rig.lightDirs[3] = rotY * normalize(vec3(0.0, 1.0, 0.0));
        rig.lightColors[3] = vec3(2.0, 1.8, 1.6);

        rig.lightDirs[4] = normalize(vec3(0.0, -1.0, 0.0));
        rig.lightColors[4] = vec3(1.0, 0.8, 0.5); // Sand bounce

        rig.ambientSky    = vec3(0.35, 0.50, 0.75);
        rig.ambientGround = vec3(0.35, 0.28, 0.18);

        rig.softboxDir1   = rig.lightDirs[0];
        rig.softboxColor1 = vec3(3.8, 3.5, 3.0);
        rig.softboxDir2   = rig.lightDirs[4];
        rig.softboxColor2 = vec3(1.2, 0.9, 0.6);
    } else {
        // Studio Neutral (Default Clean Photography Studio)
        rig.lightDirs[0] = rotY * normalize(vec3(0.65, 0.85, 0.65));
        rig.lightColors[0] = vec3(3.0, 3.0, 3.0); // 5500K Clean White Key

        rig.lightDirs[1] = rotY * normalize(vec3(-0.75, 0.45, -0.55));
        rig.lightColors[1] = vec3(1.3, 1.35, 1.45); // Soft Fill

        rig.lightDirs[2] = rotY * normalize(vec3(0.0, 0.65, -0.90));
        rig.lightColors[2] = vec3(2.2, 2.2, 2.3); // Rim

        rig.lightDirs[3] = rotY * normalize(vec3(0.0, 1.0, 0.0));
        rig.lightColors[3] = vec3(1.5, 1.5, 1.5); // Overhead Softbox

        rig.lightDirs[4] = normalize(vec3(0.0, -1.0, 0.2));
        rig.lightColors[4] = vec3(0.6, 0.6, 0.65); // Floor bounce

        rig.ambientSky    = vec3(0.35, 0.38, 0.42);
        rig.ambientGround = vec3(0.18, 0.18, 0.20);

        rig.softboxDir1   = rig.lightDirs[0];
        rig.softboxColor1 = vec3(2.5, 2.5, 2.5);
        rig.softboxDir2   = rig.lightDirs[1];
        rig.softboxColor2 = vec3(1.2, 1.3, 1.4);
    }

    return rig;
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
    // Solo Channel Viewport Modes (M / C keys)
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
        albedo = vec3(0.72);
        metallic = 0.0;
        roughness = 0.45;
    }

    vec3 V = normalize(ubo.cameraPos - inWorldPos);
    vec3 R = reflect(-V, N);
    vec3 F0 = mix(vec3(0.04), albedo, metallic);

    // 5-Light Studio Rig
    StudioRig rig = getStudioRig(ubo.envPreset, ubo.envRotation);

    vec3 Lo = vec3(0.0);

    // Evaluate all 5 studio lights with Cook-Torrance GGX
    for (int i = 0; i < 5; ++i) {
        vec3 L = rig.lightDirs[i];
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

            vec3 lightRadiance = rig.lightColors[i] * ubo.lightIntensity;
            Lo += (kD * albedo / PI + specular) * lightRadiance * NdotL;
        }
    }

    // Studio Softbox specular reflections in direction R
    float softbox1 = pow(max(dot(R, rig.softboxDir1), 0.0), mix(64.0, 4.0, roughness));
    float softbox2 = pow(max(dot(R, rig.softboxDir2), 0.0), mix(48.0, 3.0, roughness));
    vec3 softboxSpec = (rig.softboxColor1 * softbox1 + rig.softboxColor2 * softbox2) * (1.0 - roughness * 0.7);

    // Two-hemisphere ambient irradiance
    float upFraction = N.y * 0.5 + 0.5;
    vec3 ambientDiffuse = mix(rig.ambientGround, rig.ambientSky, upFraction) * 0.40;

    // Specular IBL Environment reflection
    vec3 kS_env = fresnelSchlickRoughness(max(dot(N, V), 0.0), F0, roughness);
    vec3 kD_env = (1.0 - kS_env) * (1.0 - metallic);

    float specUp = R.y * 0.5 + 0.5;
    vec3 envReflection = mix(rig.ambientGround, rig.ambientSky, specUp) * 0.5;
    vec3 specularIBL = (envReflection + softboxSpec * 0.35) * kS_env;

    vec3 ambient = (kD_env * ambientDiffuse * albedo + specularIBL) * ubo.lightIntensity;

    vec3 finalColor = ambient + Lo;

    // ACES Filmic Tone Mapping Curve (Matches Substance Painter Iray / Viewport tone mapping)
    vec3 a = finalColor * (finalColor + 0.0245786) - 0.000090537;
    vec3 b = finalColor * (0.983729 * finalColor + 0.4329510) + 0.238081;
    finalColor = clamp(a / b, 0.0, 1.0);

    // Linear to sRGB gamma correction
    finalColor = pow(finalColor, vec3(1.0 / 2.2));

    outColor = vec4(finalColor, alpha);
}
