cbuffer TransitionConstants : register(b0)
{
    float progress;
    float targetAspect;
    float oldAspect;
    float newAspect;
    float style;
    float padding0;
    float padding1;
    float padding2;
};

Texture2D oldImage : register(t0);
Texture2D newImage : register(t1);
SamplerState imageSampler : register(s0);

float hash11(float p)
{
    p = frac(p * 0.1031);
    p *= p + 33.33;
    p *= p + p;
    return frac(p);
}

float hash21(float2 p)
{
    return frac(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}

float valueNoise(float2 p)
{
    float2 cell = floor(p);
    float2 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    return lerp(lerp(hash21(cell), hash21(cell + float2(1, 0)), f.x),
                lerp(hash21(cell + float2(0, 1)), hash21(cell + 1), f.x), f.y);
}

float2 coverUv(float2 uv, float sourceAspect)
{
    if (sourceAspect > targetAspect)
        uv.x = 0.5 + (uv.x - 0.5) * targetAspect / sourceAspect;
    else
        uv.y = 0.5 + (uv.y - 0.5) * sourceAspect / targetAspect;
    return uv;
}

// Reveal a pixel whose normalized threshold lies in [0, 1]. Progress travels
// across the feather on both sides, so nothing moves at the first frame and
// every pixel has finished before the final frame.
float revealAt(float threshold, float feather)
{
    float travel = progress * (1.0 + 2.0 * feather) - feather;
    return smoothstep(threshold - feather, threshold + feather, travel);
}

struct FullOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

FullOutput VSMain(uint vertexId : SV_VertexID)
{
    FullOutput output;
    float2 position = vertexId == 0 ? float2(-1.0, -1.0)
                    : vertexId == 1 ? float2(-1.0, 3.0)
                                    : float2(3.0, -1.0);
    output.position = float4(position, 0.0, 1.0);
    output.uv = float2(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return output;
}

float4 PSMain(FullOutput input) : SV_Target
{
    float2 uv = input.uv;
    float4 oldColor = oldImage.Sample(imageSampler, coverUv(uv, oldAspect));
    float4 newColor = newImage.Sample(imageSampler, coverUv(uv, newAspect));
    if (progress <= 0.0) return float4(oldColor.rgb, 1.0);
    if (progress >= 1.0) return float4(newColor.rgb, 1.0);

    int effect = (int)(style + 0.5);
    float2 centered = uv - 0.5;
    float radius = length(float2(centered.x * targetAspect, centered.y));
    float angle = atan2(centered.y, centered.x);
    float noise = valueNoise(uv * 9.0 + progress * 2.3);
    float mask = progress;

    // Thresholds are normalized so the farthest pixel finishes before the final frame.
    float edgeRadius = radius / (0.5 * sqrt(targetAspect * targetAspect + 1.0));

    // Linear family: Crossfade, four wipes, Slices, Blinds, Checker.
    if (effect == 1)
        mask = revealAt(uv.x, 0.08);
    else if (effect == 2)
        mask = revealAt(1.0 - uv.x, 0.08);
    else if (effect == 3)
        mask = revealAt(uv.y, 0.08);
    else if (effect == 4)
        mask = revealAt(1.0 - uv.y, 0.08);
    else if (effect == 5)
        mask = step(frac(uv.y * 15.0 + floor(uv.x * 8.0) * 0.13), progress);
    else if (effect == 6)
        mask = step(frac(uv.x * 18.0), progress);
    else if (effect == 7)
        mask = step(fmod(floor(uv.x * 16.0) + floor(uv.y * 10.0), 2.0) * 0.24 + uv.y * 0.76, progress);

    // Geometric and optical family.
    else if (effect == 8)
    {
        // Square blocks that coarsen to seven rows mid-way and return to pixel
        // size at both ends, so neither endpoint snaps from a pixelated frame.
        float rows = lerp(2048.0, 7.0, pow(abs(sin(progress * 3.14159)), 0.35));
        float2 blocks = float2(rows * targetAspect, rows);
        float2 blockUv = (floor(uv * blocks) + 0.5) / blocks;
        oldColor = oldImage.Sample(imageSampler, coverUv(blockUv, oldAspect));
        newColor = newImage.Sample(imageSampler, coverUv(blockUv, newAspect));
    }
    else if (effect == 9)
    {
        float edge = progress * 1.09;
        mask = 1.0 - smoothstep(edge - 0.09, edge, edgeRadius);
    }
    else if (effect == 10)
    {
        float ringRadius = progress * 1.18;
        float ringWidth = lerp(0.18, 0.04, progress);
        float leading = 1.0 - smoothstep(ringRadius - ringWidth, ringRadius, edgeRadius);
        float trailing = 1.0 - smoothstep(max(0.0, ringRadius - ringWidth * 3.0), ringRadius - ringWidth * 2.0, edgeRadius);
        mask = max(leading, trailing * progress);
    }
    else if (effect == 11)
        mask = revealAt((radius + abs(centered.x) * 0.42)
                        / (0.5 * sqrt(targetAspect * targetAspect + 1.0) + 0.21), 0.06);
    else if (effect == 12)
        // Screen-shaped ellipse, distinct from Iris's round reveal.
        mask = revealAt(length(centered) / 0.70711, 0.05);
    else if (effect == 13)
        mask = step(0.02 + hash21(floor(uv * float2(20.0, 12.0))) * 0.96, progress);
    else if (effect == 14)
    {
        // Lower edges start no earlier than the travel (-0.05), so no cell is
        // revealed at the first frame; travel ends past the corner's upper edge.
        float2 cell = frac(uv * 14.0) - 0.5;
        mask = smoothstep(max(length(cell) + hash21(floor(uv * 14.0)) * 0.25 - 0.18, -0.05),
                          length(cell) + 0.12, lerp(-0.05, 0.83, progress));
    }
    else if (effect == 15)
    {
        float sector = abs(frac(angle / 1.0472 + 0.5) - 0.5) * 2.0;
        mask = revealAt((radius + sector * 0.23)
                        / (0.5 * sqrt(targetAspect * targetAspect + 1.0) + 0.23), 0.07);
    }

    // Distortion and blur family.
    else if (effect == 16)
    {
        float ripple = sin(radius * 55.0 - progress * 15.0) * 0.012 * (1.0 - progress);
        float2 offset = normalize(centered + 0.0001) * ripple;
        float red = newImage.Sample(imageSampler, coverUv(uv + offset, newAspect)).r;
        float blue = newImage.Sample(imageSampler, coverUv(uv - offset, newAspect)).b;
        newColor = float4(red, newColor.g, blue, 1.0);
        mask = revealAt(edgeRadius, 0.1);
    }
    else if (effect == 17)
    {
        float row = floor(uv.y * 36.0);
        float jitter = (hash11(row + floor(progress * 20.0)) - 0.5) * 0.12 * sin(progress * 3.14159);
        newColor = newImage.Sample(imageSampler, coverUv(uv + float2(jitter, 0.0), newAspect));
        mask = step(0.02 + hash11(row * 3.1) * 0.96, progress);
    }
    else if (effect == 18)
    {
        float2 warped = uv + (centered * 0.19 + sin(uv.yx * 18.0) * 0.015) * sin(progress * 3.14159);
        newColor = newImage.Sample(imageSampler, coverUv(warped, newAspect));
    }
    else if (effect == 19 || effect == 20)
    {
        float4 blurred = 0.0;
        [unroll] for (int sampleIndex = 0; sampleIndex < 5; sampleIndex++)
        {
            float amount = (sampleIndex - 2) * 0.025 * sin(progress * 3.14159);
            float2 sampleUv = effect == 19
                ? uv + centered * amount
                : uv + normalize(centered + 0.001) * amount;
            blurred += newImage.Sample(imageSampler, coverUv(sampleUv, newAspect));
        }
        newColor = blurred / 5.0;
    }
    else if (effect == 21)
    {
        float spin = (1.0 - progress) * 0.8 * exp(-radius * 2.8);
        float2 rotated = float2(cos(spin) * centered.x - sin(spin) * centered.y,
                                sin(spin) * centered.x + cos(spin) * centered.y) + 0.5;
        newColor = newImage.Sample(imageSampler, coverUv(rotated, newAspect));
    }
    else if (effect == 22)
    {
        float2 displaced = uv + float2(noise - 0.5, valueNoise(uv * 11.0) - 0.5)
                           * 0.12 * sin(progress * 3.14159);
        newColor = newImage.Sample(imageSampler, coverUv(displaced, newAspect));
    }
    else if (effect == 23)
        mask = revealAt((uv.x + sin(uv.y * 18.0) * 0.09 + 0.09) / 1.18, 0.08);

    // Organic and procedural family.
    else if (effect == 24)
        mask = revealAt(noise, 0.12);
    else if (effect == 25)
        mask = revealAt(valueNoise(uv * 5.0 + noise), 0.18);
    else if (effect == 26)
        mask = revealAt(0.5 * (valueNoise(uv * 3.0 + float2(0.0, progress * 2.0))
                               + valueNoise(uv * 3.0)), 0.24);
    else if (effect == 27)
        mask = smoothstep(noise - 0.08, noise + 0.05, progress * 1.13 - 0.08);
    else if (effect == 28)
    {
        float luma = dot(oldColor.rgb, float3(0.2126, 0.7152, 0.0722));
        mask = revealAt(luma, 0.12);
    }
    else if (effect == 29)
        mask = revealAt(hash11(floor(uv.x * 220.0)) * 0.7 + uv.y * 0.3, 0.08);

    return float4(lerp(oldColor.rgb, newColor.rgb, saturate(mask)), 1.0);
}
