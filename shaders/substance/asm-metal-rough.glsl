/*************************************************************************
* ADOBE CONFIDENTIAL
* ___________________
* Copyright 2010-2020 Adobe
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

//- Adobe Standard Material Definition (ASM) shader
//- ===============================================
//-
//- Import from libraries.
import lib-pbr.glsl
import lib-pbr-aniso.glsl

//: param custom {
//:   "group": "Base Surface",
//:   "label": "Enable anisotropy",
//:   "default": false,
//:   "asm": "anisotropy",
//:   "description": "<html><head/><body><p>Allows reflections to stretch in one direction along the surface.<br/><b>Please note</b>: The following channels need to be present for this parameter to have an effect: <b>Anisotropy angle</b> and <b>Anisotropy level</b>.</p></body></html>"
//: }
uniform_specialization bool anisotropyEnabled;
//: param custom {
//:   "group": "Base Surface",
//:   "label": "Index of refraction",
//:   "min": 0.0,
//:   "max": 40.0,
//:   "default": 1.5,
//:   "asm": "specular_ior",
//:   "description": "<html><head/><body><p>The amount light bends as it passes through the object. Also affects the specular reflection intensity.</p></body></html>"
//: }
uniform float specularIoR;
//: param custom {
//:   "group": "Base Surface",
//:   "label": "Enable edge color",
//:   "default": false,
//:   "asm": "specular_edge_color",
//:   "description": "<html><head/><body><p>Allows specifying the color of light reflections. Affects glancing angles for metallic materials.<br/><b>Please note</b>: The following channel needs to be present for this parameter to have an effect: <b>Specular edge color</b></p></body></html>"
//: }
uniform_specialization bool specularEdgeColorEnabled;

import lib-sheen.glsl

//: param custom {
//:   "group": "Geometry",
//:   "label": "Double sided",
//:   "default": false,
//:   "description": "<html><head/><body><p>When enabled, the surface is visible on both sides, i.e. back-face culling is disabled.</p></body></html>"
//: }
uniform_specialization bool doubleSided;

import lib-pom.glsl
import lib-bent-normal.glsl

//: param custom {
//:   "group": "Interior",
//:   "label": "Enable translucency",
//:   "default": false,
//:   "asm": "translucency",
//:   "description": "<html><head/><body><p>Allows for refractive transmission of light through the surface.<br/><b>Please note</b>: The following channel needs to be present for this parameter to have an effect: <b>Translucency</b></p></body></html>",
//:   "enable": "!input.sssEnabled",
//:   "description_disabled": "<html><head/><body><p>Disable the <b>Subsurface Scattering</b> to use <b>Translucency</b>.</p></body></html>"
//: }
uniform_specialization bool translucencyEnabled;

import lib-sss.glsl

//: param custom {
//:   "group": "Interior",
//:   "label": "Enable absorption",
//:   "default": false,
//:   "asm": "absorption_color",
//:   "description": "<html><head/><body><p>Filters the light that passes through the volume by absorbing certain colors. Affects both translucency and subsurface scattering.<br/><b>Please note</b>: The following channel needs to be present for this parameter to have an effect: <b>Absorption color</b></p></body></html>",
//:   "visible": "input.translucencyEnabled || input.sssEnabled"
//: }
uniform_specialization bool absorptionEnabled;

import lib-alpha-test.glsl

//: param custom {
//:   "group": "Geometry/Opacity",
//:   "label": "Enable alpha blending",
//:   "default": false,
//:   "asm": "opacity",
//:   "description": "<html><head/><body><p>Uses the opacity texture to progressively blend the transparent surface over the background.<br/><b>Please note</b>: The following channel needs to be present for this parameter to have an effect: <b>Opacity</b></p></body></html>",
//:   "enable": "!input.sssEnabled",
//:   "description_disabled": "<html><head/><body><p>Disable the <b>Subsurface Scattering</b> to use <b>Alpha blending</b>.</p></body></html>"
//: }
uniform_specialization bool alphaBlendEnabled;

import lib-coat.glsl
import lib-utils.glsl

//- Declare the iray mdl material to use with this shader.
//: metadata {
//:   "mdl":"mdl::alg::materials::painter::standard"
//: }

//- Disable culling if double sided option is enabled
//: state cull_face off {"enable":"input.doubleSided"}
//- Enable 'over' alpha blending if opacity alpha blend
//: state blend over {"enable":"input.alphaBlendEnabled && !input.sssEnabled && !input.translucencyEnabled"}
//- Enable 'premultiplied over' alpha blending if translucency
//: state blend over_premult {"enable":"input.translucencyEnabled && !input.absorptionEnabled && !input.sssEnabled"}
//- Enable 'add multiply' alpha blending if absorption is required
//: state blend add_multiply {"enable":"input.translucencyEnabled && input.absorptionEnabled && !input.sssEnabled"}

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
//: param auto channel_specularlevel
uniform SamplerSparse specularlevel_tex;
//: param auto channel_specularedgecolor
uniform SamplerSparse specularedgecolor_tex;
//: param auto channel_absorptioncolor
uniform SamplerSparse absorptioncolor_tex;

//- Shader entry point.
void shade(V2F inputs)
{
	// Apply parallax occlusion mapping if possible
	vec3 viewTS = worldSpaceToTangentSpace(getEyeVec(inputs.position), inputs);
	applyParallaxOffset(inputs, viewTS);

	// Fetch material parameters, and conversion to the specular/roughness model
	float baseRoughness = getRoughness(roughness_tex, inputs.sparse_coord);

	float anisotropyLevel = 0.0;
	float anisotropyAngle = 0.0;
	if (anisotropyEnabled) {
		anisotropyLevel = getAnisotropyLevel(anisotropylevel_tex, inputs.sparse_coord);
		if (anisotropyLevel > 0.0) {
			anisotropyAngle = getAnisotropyAngle(anisotropyangle_tex, inputs.sparse_coord);
		}
	}

	vec3 baseColor = getBaseColor(basecolor_tex, inputs.sparse_coord);
	float metallic = getMetallic(metallic_tex, inputs.sparse_coord);
	vec3 diffColor = generateDiffuseColor(baseColor, metallic);
	float specularLevel = getSpecularLevel(specularlevel_tex, inputs.sparse_coord);
	vec3 specColor_metal = baseColor;
	float dielectricF0 = iorToSpecularLevel(1.0, specularIoR);
	float coatOpacity = 0.0;
	if (coatEnabled) {
		coatOpacity = getCoatOpacity(coatOpacity_tex, inputs.sparse_coord);
		if (coatOpacity > 0.0) {
			float underCoatF0 = iorToSpecularLevel(coatIoR, specularIoR);
			dielectricF0 = mix(dielectricF0, underCoatF0, coatOpacity);
		}
	}
	vec3 specColor_dielectric = vec3(dielectricF0 * 2.0 * specularLevel);

	// Get detail (ambient occlusion) and global (shadow) occlusion factors
	// separately in order to blend the bent normals properly
	float shadowFactor = getShadowFactor();
	float occlusion = getAO(inputs.sparse_coord, true, use_bent_normal);

	float specOcclusion = specularOcclusionCorrection(
		use_bent_normal ? shadowFactor : occlusion * shadowFactor,
		metallic,
		baseRoughness);

	vec3 normal = computeWSNormal(inputs.sparse_coord,
		inputs.tangent, inputs.bitangent, inputs.normal);
	LocalVectors vectors = computeLocalFrame(inputs, normal, anisotropyAngle);
	computeBentNormal(vectors,inputs);

	// Diffuse lobe:
	vec3 diffuseShading = occlusion * shadowFactor * envIrradiance(getDiffuseBentNormal(vectors));

	// Specular lobes:
	// The specs can be interpreted as interpolating between two lobes.
	// However, due to the prohibitive cost for real-time, we interpolate
	// between two specular colors and use it for a single specular lobe.
	vec3 specColor = mix(specColor_dielectric, specColor_metal, metallic);
	vec3 specSecondaryColor = vec3(1.0);
	if (specularEdgeColorEnabled) {
		specSecondaryColor = getSpecularEdgeColor(specularedgecolor_tex, inputs.sparse_coord);
	}
	vec3 specEdgeColor = mix(vec3(1.0), specSecondaryColor, metallic);
	vec3 specColoring = mix(vec3(1.0), specSecondaryColor, 1.0 - metallic);
	vec3 specReflection = vec3(0.0);
	if (anisotropyEnabled) {
		vec2 roughnessAniso = generateAnisotropicRoughnessASM(baseRoughness, anisotropyLevel);
		specReflection = pbrComputeSpecularAnisotropic(
			vectors,
			specColor,
			specEdgeColor,
			roughnessAniso,
			occlusion,
			getBentNormalSpecularAmount());
	}
	else {
		specReflection = pbrComputeSpecular(
			vectors,
			specColor,
			specEdgeColor,
			baseRoughness,
			occlusion,
			getBentNormalSpecularAmount());
	}
	vec3 specularShading = specOcclusion * specColoring * specReflection;

	// Sheen:
	if (sheenEnabled) {
		float sheenOpacity = getSheenOpacity(sheenOpacity_tex, inputs.sparse_coord);
		if (sheenOpacity > 0.0) {
			float sheenRoughness = getSheenRoughness(sheenRoughness_tex, inputs.sparse_coord);
			vec3 sheenColor = sheenOpacity * getSheenColor(sheenColor_tex, inputs.sparse_coord);
			vec3 sheenSpecularShading = pbrComputeSheen(vectors, sheenColor, sheenRoughness);
			specularShading += specOcclusion * sheenSpecularShading;
		}
	}

	// Coating:
	// Keep track of coat absorption quantity to know how much to reduce contribution for lower layers
	vec3 coatAbsorption = vec3(1.0);
	if (coatOpacity > 0.0) {
		vec3 coatNormal = getWSCoatNormal(inputs.sparse_coord,
			inputs.tangent, inputs.bitangent, inputs.normal);
		LocalVectors coatVectors = computeLocalFrame(inputs, coatNormal, 0.0);

		vec3 coatColor = getCoatColor(coatColor_tex, inputs.sparse_coord);
		float coatSpecularLevel = getCoatSpecularLevel(coatSpecularLevel_tex, inputs.sparse_coord);
		vec3 coatSpecColor = vec3(iorToSpecularLevel(1.0, coatIoR) * 2.0 * coatSpecularLevel);

		float coatRoughness = getCoatRoughness(coatRoughness_tex, inputs.sparse_coord);
		float coatSpecOcclusion = specularOcclusionCorrection(occlusion * shadowFactor, 0.0, coatRoughness);
		vec3 coatSpecularShading = pbrComputeSpecular(coatVectors, coatSpecColor, coatRoughness);

		float ndv = clamp(dot(coatVectors.normal, vectors.eye), 1e-4, 1.0);
		coatAbsorption = coatPassageColorMultiplier(coatColor, coatOpacity, ndv);
		coatAbsorption *= coatAbsorption;

		diffuseShading *= coatAbsorption;
		specularShading *= coatAbsorption;
		specularShading += (coatSpecOcclusion * coatOpacity) * coatSpecularShading;
	}

	albedoOutput(diffColor);

	vec3 emissiveShading = pbrComputeEmissive(emissive_tex, inputs.sparse_coord);

	// Blending related effects
	if ((translucencyEnabled || alphaBlendEnabled) && !sssEnabled) {
		float alpha = 1.0;
		// Opacity cut-through
		if (alphaBlendEnabled) {
			alpha = getOpacity(opacity_tex, inputs.sparse_coord);
		}

		// Refractive transmission
		if (translucencyEnabled) {
			// Blending premultiplied over: must premultiply contributions manually
			emissiveShading *= alpha;
			specularShading *= alpha;
			diffuseShading *= alpha;

			float translucency = getTranslucency(sss_translucency_tex, inputs.sparse_coord) * (1.0-metallic);
			diffuseShading *= 1.0-translucency;

			vec3 absorption = vec3(1.0);
			if (absorptionEnabled) {
				// Absorption use dual output blending mode
				absorption = coatAbsorption * getAbsorptionColor(absorptioncolor_tex, inputs.sparse_coord);
				color1Output(vec4(vec3(1.0)-alpha*(vec3(1.0)-absorption*translucency*(1.0-baseRoughness)), 1.0));
			}

			// Approximation: roughness cutoff the transparency
			diffuseShading += absorption * alpha * baseRoughness * translucency * envIrradiance(-vectors.normal);
			
			alpha *= 1.0-(translucency*(1.0-baseRoughness));
		}

		alphaOutput(alpha);
	}

	emissiveColorOutput(emissiveShading);
	diffuseShadingOutput(diffuseShading);
	specularShadingOutput(specularShading);

	sssCoefficientsOutput(getSSSCoefficients(inputs.sparse_coord));

	vec4 baseSSSColor = getSSSColor(inputs.sparse_coord);
	if (usesSSSScatteringColorChannel()) {
		// Must be dimmed by metallic factor as for diffuse color
		baseSSSColor.rgb = generateDiffuseColor(baseSSSColor.rgb, metallic);
	}
	if (sssEnabled && absorptionEnabled) {
		// Combine absorption color with scattering color if both enabled
		baseSSSColor.rgb *= getAbsorptionColor(absorptioncolor_tex, inputs.sparse_coord);
	}
	sssColorOutput(baseSSSColor);

	// Discard current fragment on the basis of the opacity channel
	// and a user defined threshold
	alphaKill(inputs.sparse_coord);
}
