// ============================================================================
// ACES ReShade Effect — Generated from BuildLang
// ============================================================================
// Source: aces/rrt.bld
// Built with: buildc build aces/rrt.bld --target hlsl
//
// Install: Copy to reshade-shaders/Shaders/
// ============================================================================

#include "ReShade.fxh"

// --- Begin BuildLang Generated Code ---

// Constants
static const float RRT_A = 2.51;
static const float RRT_B = 0.03;
static const float RRT_C = 2.43;
static const float RRT_D = 0.59;
static const float RRT_E = 0.14;
static const float SRGB_CUTOFF = 0.0031308;
static const float SRGB_SLOPE = 12.92;

// ACES tone mapping (from BuildLang)
float aces_tonemap(float x) {
    float num = x * (RRT_A * x + RRT_B);
    float den = x * (RRT_C * x + RRT_D) + RRT_E;
    return num / den;
}

float3 rrt(float3 scene) {
    return float3(
        aces_tonemap(scene.r),
        aces_tonemap(scene.g),
        aces_tonemap(scene.b)
    );
}

float srgb_oetf(float linear) {
    if (linear <= SRGB_CUTOFF)
        return linear * SRGB_SLOPE;
    else
        return 1.055 * pow(linear, 0.4166667) - 0.055;
}

// --- End BuildLang Generated Code ---

// ReShade UI
uniform float Exposure <
    ui_type = "slider";
    ui_min = -5.0; ui_max = 5.0;
    ui_label = "Exposure (stops)";
> = 0.0;

uniform float Saturation <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 2.0;
    ui_label = "Saturation";
> = 1.0;

// ReShade pixel shader
float4 PS_ACES(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target {
    float3 color = tex2D(ReShade::BackBuffer, uv).rgb;

    // Reverse sRGB gamma to get linear light
    color = pow(max(color, 0.0), 2.2);

    // Apply exposure
    color *= pow(2.0, Exposure);

    // Apply saturation
    float lum = dot(color, float3(0.2126, 0.7152, 0.0722));
    color = lum + (color - lum) * Saturation;

    // ACES RRT (tone mapping)
    color = rrt(color);
    color = saturate(color);

    // sRGB output
    color = float3(
        srgb_oetf(color.r),
        srgb_oetf(color.g),
        srgb_oetf(color.b)
    );

    return float4(color, 1.0);
}

technique ACES_BuildLang <
    ui_label = "ACES (BuildLang)";
    ui_tooltip = "Academy Color Encoding System tone mapping.\nWritten in BuildLang, compiled to HLSL.";
> {
    pass {
        VertexShader = PostProcessVS;
        PixelShader = PS_ACES;
    }
}
