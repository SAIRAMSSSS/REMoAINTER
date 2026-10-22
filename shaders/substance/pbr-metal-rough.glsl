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

//- Substance 3D Painter Metal/Rough PBR shader
//- ====================================
//-
//- Import from libraries.
import lib-pbr.glsl
import lib-bent-normal.glsl
import lib-emissive.glsl
import lib-pom.glsl
import lib-sss.glsl
import lib-utils.glsl

//- Declare the iray mdl material to use with this shader.
//: metadata {
//:   "mdl":"mdl::alg::materials::skin_metallic_roughness::skin_metallic_roughness"
//: }

//- Channels needed for metal/rough workflow are bound here.
//: param auto channel_basecolor
uniform SamplerSparse basecolor_tex;
//: param auto channel_roughness
uniform SamplerSparse roughness_tex;
//: param auto channel_metallic
uniform SamplerSparse metallic_tex;
//: param auto channel_specularlevel
uniform SamplerSparse specularlevel_tex;

//- Shader entry point.
void shade(V2F inputs)
{
  // Apply parallax occlusion mapping if possible
  vec3 viewTS = worldSpaceToTangentSpace(getEyeVec(inputs.position), inputs);
  applyParallaxOffset(inputs, viewTS);

  // Fetch material parameters, and conversion to the specular/roughness model
  float roughness = getRoughness(roughness_tex, inputs.sparse_coord);
  vec3 baseColor = getBaseColor(basecolor_tex, inputs.sparse_coord);
  float metallic = getMetallic(metallic_tex, inputs.sparse_coord);
  float specularLevel = getSpecularLevel(specularlevel_tex, inputs.sparse_coord);
  vec3 diffColor = generateDiffuseColor(baseColor, metallic);
  vec3 specColor = generateSpecularColor(specularLevel, baseColor, metallic);

  // Get detail (ambient occlusion) and global (shadow) occlusion factors
  // separately in order to blend the bent normals properly
  float shadowFactor = getShadowFactor();
  float occlusion = getAO(inputs.sparse_coord, true, use_bent_normal);
  float specOcclusion = specularOcclusionCorrection(
    use_bent_normal ? shadowFactor : occlusion * shadowFactor,
    metallic,
    roughness);

  LocalVectors vectors = computeLocalFrame(inputs);
  computeBentNormal(vectors,inputs);

  // Feed parameters for a physically based BRDF integration
  emissiveColorOutput(pbrComputeEmissive(emissive_tex, inputs.sparse_coord));
  albedoOutput(diffColor);
  diffuseShadingOutput(occlusion * shadowFactor * envIrradiance(getDiffuseBentNormal(vectors)));
  specularShadingOutput(specOcclusion * pbrComputeSpecular(vectors, specColor, roughness, occlusion, getBentNormalSpecularAmount()));
  sssCoefficientsOutput(getSSSCoefficients(inputs.sparse_coord));

  vec4 baseSSSColor = getSSSColor(inputs.sparse_coord);
  if (usesSSSScatteringColorChannel()) {
    // Must be dimmed by metallic factor as for diffuse color
    baseSSSColor.rgb = generateDiffuseColor(baseSSSColor.rgb, metallic);
  }
  sssColorOutput(baseSSSColor);
}
