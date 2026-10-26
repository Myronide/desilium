// Shared prelude for all pixel-shader effects (prepended on the CPU side).
// All geometry is expressed in PHYSICAL pixels so edges land on the pixel grid
// of the output regardless of the fractional scale.
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

varying vec2 v_coords;
uniform vec2 size;
uniform float alpha;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

// Signed distance to a rounded box centred at the origin (y down).
// radii = (top-left, top-right, bottom-right, bottom-left).
float sd_rounded_box(vec2 p, vec2 half_size, vec4 radii) {
    float r = p.x > 0.0 ? (p.y > 0.0 ? radii.z : radii.y)
                        : (p.y > 0.0 ? radii.w : radii.x);
    r = min(r, min(half_size.x, half_size.y));
    vec2 q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

// One-pixel wide linear coverage ramp centred on the edge (d is in pixels).
float coverage(float d) {
    return clamp(0.5 - d, 0.0, 1.0);
}

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Triangular-PDF noise, +-1 LSB of an 8 bit channel: removes banding without visible grain.
float dither_noise() {
    vec2 p = gl_FragCoord.xy;
    return (hash12(p) + hash12(p + vec2(17.0, 43.0)) - 1.0) * (1.0 / 255.0);
}

// Abramowitz-Stegun style erf approximation (max error ~5e-4), vectorised.
vec2 erf2(vec2 x) {
    vec2 s = sign(x);
    vec2 a = abs(x);
    vec2 t = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    t *= t;
    return s - s / (t * t);
}

// Applies renderer alpha and debug tint, then writes the premultiplied result.
void finish(vec4 color) {
    color *= alpha;
#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif
    gl_FragColor = color;
}
