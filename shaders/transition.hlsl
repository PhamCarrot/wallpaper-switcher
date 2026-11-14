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

    // Linear family: Crossfade, four wipes, Slices, Blinds, Checker.
    if (effect == 1)
        mask = 1.0 - smoothstep(progress - 0.08, progress + 0.08, uv.x);
    else if (effect == 2)
        mask = smoothstep(1.0 - progress - 0.08, 1.0 - progress + 0.08, uv.x);
    else if (effect == 3)
        mask = 1.0 - smoothstep(progress - 0.08, progress + 0.08, uv.y);
    else if (effect == 4)
        mask = smoothstep(1.0 - progress - 0.08, 1.0 - progress + 0.08, uv.y);
    else if (effect == 5)
        mask = step(frac(uv.y * 15.0 + floor(uv.x * 8.0) * 0.13), progress);
    else if (effect == 6)
        mask = step(frac(uv.x * 18.0), progress);
    else if (effect == 7)
        mask = step(fmod(floor(uv.x * 16.0) + floor(uv.y * 10.0), 2.0) * 0.24 + uv.y * 0.76, progress);

    // Geometric and optical family.
    else if (effect == 8)
    {
        float blocks = lerp(90.0, 7.0, sin(progress * 3.14159));
        float2 blockUv = (floor(uv * blocks) + 0.5) / blocks;
        oldColor = oldImage.Sample(imageSampler, coverUv(blockUv, oldAspect));
        newColor = newImage.Sample(imageSampler, coverUv(blockUv, newAspect));
    }
    else if (effect == 9)
        mask = 1.0 - smoothstep(progress * 0.82, progress * 0.82 + 0.09, radius);
    else if (effect == 10)
    {
        float ringRadius = progress * 0.92;
        float ringWidth = lerp(0.18, 0.04, progress);
        float leading = 1.0 - smoothstep(ringRadius - ringWidth, ringRadius, radius);
        float trailing = 1.0 - smoothstep(max(0.0, ringRadius - ringWidth * 3.0), ringRadius - ringWidth * 2.0, radius);
        mask = max(leading, trailing * progress);
    }
    else if (effect == 11)
        mask = smoothstep(radius + abs(centered.x) * 0.42 - 0.06,
                          radius + abs(centered.x) * 0.42 + 0.06, progress);
    else if (effect == 12)
        mask = 1.0 - smoothstep(progress * 0.9 - 0.05, progress * 0.9 + 0.05, radius);
    else if (effect == 13)
        mask = step(hash21(floor(uv * float2(20.0, 12.0))), progress);
    else if (effect == 14)
    {
        float2 cell = frac(uv * 14.0) - 0.5;
        mask = smoothstep(length(cell) + hash21(floor(uv * 14.0)) * 0.25 - 0.18,
                          length(cell) + 0.12, progress);
    }
    else if (effect == 15)
    {
        float sector = abs(frac(angle / 1.0472 + 0.5) - 0.5) * 2.0;
        mask = smoothstep(radius + sector * 0.23 - 0.07,
                          radius + sector * 0.23 + 0.07, progress);
    }

    // Distortion and blur family.
    else if (effect == 16)
    {
        float ripple = sin(radius * 55.0 - progress * 15.0) * 0.012 * (1.0 - progress);
        float2 offset = normalize(centered + 0.0001) * ripple;
        float red = newImage.Sample(imageSampler, coverUv(uv + offset, newAspect)).r;
        float blue = newImage.Sample(imageSampler, coverUv(uv - offset, newAspect)).b;
        newColor = float4(red, newColor.g, blue, 1.0);
        mask = 1.0 - smoothstep(progress * 0.9 - 0.1, progress * 0.9 + 0.1, radius);
    }
    else if (effect == 17)
    {
        float row = floor(uv.y * 36.0);
        float jitter = (hash11(row + floor(progress * 20.0)) - 0.5) * 0.12 * sin(progress * 3.14159);
        newColor = newImage.Sample(imageSampler, coverUv(uv + float2(jitter, 0.0), newAspect));
        mask = step(hash11(row * 3.1), progress);
    }
    else if (effect == 18)
    {
        float2 warped = uv + centered * sin(progress * 3.14159) * 0.19 + sin(uv.yx * 18.0) * 0.015;
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
        mask = smoothstep(uv.x + sin(uv.y * 18.0) * 0.09 - 0.08,
                          uv.x + sin(uv.y * 18.0) * 0.09 + 0.08, progress);

    // Organic and procedural family.
    else if (effect == 24)
        mask = smoothstep(noise - 0.12, noise + 0.12, progress);
    else if (effect == 25)
        mask = smoothstep(valueNoise(uv * 5.0 + noise) - 0.18,
                          valueNoise(uv * 5.0 + noise) + 0.18, progress);
    else if (effect == 26)
        mask = smoothstep(valueNoise(uv * 3.0 + float2(0.0, progress * 2.0)) - 0.24,
                          valueNoise(uv * 3.0) + 0.24, progress);
    else if (effect == 27)
        mask = smoothstep(noise - 0.08, noise + 0.05, progress);
    else if (effect == 28)
    {
        float luma = dot(oldColor.rgb, float3(0.2126, 0.7152, 0.0722));
        mask = smoothstep(luma - 0.12, luma + 0.12, progress);
    }
    else if (effect == 29)
        mask = smoothstep(hash11(floor(uv.x * 220.0)) * 0.7 + uv.y * 0.3 - 0.08,
                          hash11(floor(uv.x * 220.0)) * 0.7 + uv.y * 0.3 + 0.08,
                          progress);

    return float4(lerp(oldColor.rgb, newColor.rgb, saturate(mask)), 1.0);
}
