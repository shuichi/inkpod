#include <metal_stdlib>
using namespace metal;

struct InkpodCanvasVertex {
    float2 position;
    float2 textureCoordinate;
};

struct InkpodCanvasVertexOut {
    float4 position [[position]];
    float2 textureCoordinate;
};

vertex InkpodCanvasVertexOut inkpodCanvasVertex(
    const device InkpodCanvasVertex *vertices [[buffer(0)]],
    constant float2 &viewport [[buffer(1)]],
    uint vertexID [[vertex_id]])
{
    InkpodCanvasVertexOut output;
    const float2 pixel = vertices[vertexID].position;
    output.position = float4(
        (pixel.x / viewport.x) * 2.0 - 1.0,
        1.0 - (pixel.y / viewport.y) * 2.0,
        0.0,
        1.0);
    output.textureCoordinate = vertices[vertexID].textureCoordinate;
    return output;
}

fragment float4 inkpodCanvasFragment(
    InkpodCanvasVertexOut input [[stage_in]],
    texture2d<float> source [[texture(0)]],
    sampler sourceSampler [[sampler(0)]])
{
    return source.sample(sourceSampler, input.textureCoordinate);
}

fragment float4 inkpodCanvasOpacityFragment(
    InkpodCanvasVertexOut input [[stage_in]],
    texture2d<float> source [[texture(0)]],
    sampler sourceSampler [[sampler(0)]],
    constant float &opacity [[buffer(0)]])
{
    return source.sample(sourceSampler, input.textureCoordinate) * opacity;
}

fragment float4 inkpodCanvasSolidFragment(
    InkpodCanvasVertexOut input [[stage_in]],
    constant float4 &color [[buffer(0)]])
{
    return color;
}

fragment float4 inkpodCanvasLUTFragment(
    InkpodCanvasVertexOut input [[stage_in]],
    texture2d<float> source [[texture(0)]],
    sampler sourceSampler [[sampler(0)]],
    const device uchar *lut [[buffer(0)]])
{
    const float4 value = source.sample(sourceSampler, input.textureCoordinate);
    const uint red = min(uint(round(value.r * 255.0)), 255u);
    const uint green = min(uint(round(value.g * 255.0)), 255u);
    const uint blue = min(uint(round(value.b * 255.0)), 255u);
    return float4(
        float(lut[red]) / 255.0,
        float(lut[256u + green]) / 255.0,
        float(lut[512u + blue]) / 255.0,
        value.a);
}
