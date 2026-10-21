/*************************************************************************
* ADOBE CONFIDENTIAL
* ___________________
* Copyright 2010-2014 Adobe
* All Rights Reserved.
* NOTICE:  All information contained herein is, and remains
* the property of Adobe and its suppliers, if any. The intellectual
* and technical concepts contained herein are proprietary to Adobe
* and its suppliers and are protected by all applicable intellectual
* property laws, including trade secret and copyright laws.
* Dissemination of this information or reproduction of this material
* is strictly forbidden unless prior written permission is obtained
* from Adobe.
*************************************************************************/

//- Substance 3D Painter Metal/Rough Anisotropy PBR shader
//- ===============================================
//-
//- Import from libraries.
import lib-pbr.glsl
import lib-pbr-aniso.glsl
import lib-bent-normal.glsl
import lib-emissive.glsl
import lib-sss.glsl
import lib-alpha.glsl
import lib-utils.glsl

//- Declare the iray mdl material to use with this shader.
//: metadata {
//:   "mdl":"mdl::alg::materials::skin_metallic_roughness::skin_metallic_roughness"
//: }

//- Show back faces as there may be holes in front faces.
//: state cull_face off

//- Channels needed for metal/rough workflow are bound here.
//: param auto channel_basecolor
uniform SamplerSparse basecolor_tex;
//: param auto channel_roughness
uniform SamplerSparse roughness_tex;
//: param auto channel_metallic
uniform SamplerSparse metallic_tex;
//: param auto channel_anisotropylevel
uniform SamplerSparse anisotropylevel_tex;
//: param auto channel_anisotropyangle
uniform SamplerSparse anisotropyangle_tex;

//- Shader entry point.
void shade(V2F inputs)
{

  // Fetch material parameters, and conversion to the specular/roughness model
  float roughness = getRoughness(roughness_tex, inputs.sparse_coord);
  float anisotropyLevel = getAnisotropyLevel(anisotropylevel_tex, inputs.sparse_coord);
  vec2 roughnessAniso = generateAnisotropicRoughness(roughness, anisotropyLevel);
  float anisotropyAngle = getAnisotropyAngle(anisotropyangle_tex, inputs.sparse_coord);

  vec3 baseColor = getBaseColor(basecolor_tex, inputs.sparse_coord);
  float metallic = getMetallic(metallic_tex, inputs.sparse_coord);

  // Get detail (ambient occlusion) and global (shadow) occlusion factors
  // separately in order to blend the bent normals properly
  float shadowFactor = getShadowFactor();
  float occlusion = getAO(inputs.sparse_coord, true, use_bent_normal);
  float specOcclusion = specularOcclusionCorrection(
    use_bent_normal ? shadowFactor : occlusion * shadowFactor,
    metallic,
    roughness);

  vec3 normal = computeWSNormal(inputs.sparse_coord, inputs.tangent, inputs.bitangent, inputs.normal);
  LocalVectors vectors = computeLocalFrame(inputs, normal, anisotropyAngle);
  computeBentNormal(vectors,inputs);

  // Feed parameters for a physically based BRDF integration
  alphaOutput(getOpacity(opacity_tex, inputs.sparse_coord));
  emissiveColorOutput(pbrComputeEmissive(emissive_tex, inputs.sparse_coord));
  sssCoefficientsOutput(getSSSCoefficients(inputs.sparse_coord));

  vec4 baseSSSColor = getSSSColor(inputs.sparse_coord);
  if (usesSSSScatteringColorChannel()) {
    // Must be dimmed by metallic factor as for diffuse color
    baseSSSColor.rgb = generateDiffuseColor(baseSSSColor.rgb, metallic);
  }
  sssColorOutput(baseSSSColor);

  // Discard current fragment on the basis of the opacity channel
  // and a user defined threshold
  alphaKill(inputs.sparse_coord);

  vec3 diffColor = generateDiffuseColor(baseColor, metallic);
  albedoOutput(diffColor);
  diffuseShadingOutput(occlusion * shadowFactor * envIrradiance(getDiffuseBentNormal(vectors)));
  vec3 specColor = generateSpecularColor(baseColor, metallic);
  specularShadingOutput(specOcclusion * pbrComputeSpecularAnisotropic(vectors, specColor, roughnessAniso, occlusion, getBentNormalSpecularAmount()));
}
