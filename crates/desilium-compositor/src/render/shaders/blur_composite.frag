// Draws the blurred backdrop clipped to a rounded rectangle, with saturation boost,
// tint (the translucent surface fill) and a faint static noise against banding.
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

uniform sampler2D tex;
uniform vec2 u_tex;          // allocated size of the blurred texture (texels)
uniform vec2 u_content;      // valid region (texels)
uniform vec2 u_grab_origin;  // framebuffer pixel that maps to texel (0,0) of level 0
uniform float u_inv_level;   // 1 / 2^downsamples: framebuffer pixels -> blurred texels
uniform float u_cubic;       // 1.0: smooth B-spline up-sampling, 0.0: plain bilinear
uniform vec4 u_rect;         // glass rectangle (framebuffer px, y up): x, y, w, h
uniform vec4 u_radii;        // bl, br, tr, tl in framebuffer px
uniform float u_saturation;
uniform vec4 u_tint;         // premultiplied
uniform float u_opacity;
uniform float u_noise;

vec4 fetch(vec2 coord) {
    coord = clamp(coord, vec2(0.5), u_content - 0.5);
    return texture2D(tex, coord / u_tex);
}

// Cubic B-spline filter built from four bilinear fetches (Sigg & Hadwiger).
vec4 fetch_cubic(vec2 coord) {
    vec2 grid = coord - 0.5;
    vec2 base = floor(grid);
    vec2 f = grid - base;
    vec2 f2 = f * f;
    vec2 f3 = f2 * f;
    vec2 w0 = (1.0 - f) * (1.0 - f) * (1.0 - f) / 6.0;
    vec2 w1 = (3.0 * f3 - 6.0 * f2 + 4.0) / 6.0;
    vec2 w2 = (-3.0 * f3 + 3.0 * f2 + 3.0 * f + 1.0) / 6.0;
    vec2 w3 = f3 / 6.0;
    vec2 g0 = w0 + w1;
    vec2 g1 = w2 + w3;
    vec2 p0 = base + 0.5 - 1.0 + w1 / g0;
    vec2 p1 = base + 0.5 + 1.0 + w3 / g1;
    vec4 row0 = g0.x * fetch(vec2(p0.x, p0.y)) + g1.x * fetch(vec2(p1.x, p0.y));
    vec4 row1 = g0.x * fetch(vec2(p0.x, p1.y)) + g1.x * fetch(vec2(p1.x, p1.y));
    return g0.y * row0 + g1.y * row1;
}

float sd_rounded_box(vec2 p, vec2 half_size, vec4 radii) {
    float r = p.x > 0.0 ? (p.y > 0.0 ? radii.z : radii.y)
                        : (p.y > 0.0 ? radii.w : radii.x);
    r = min(r, min(half_size.x, half_size.y));
    vec2 q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

void main() {
    vec2 pixel = gl_FragCoord.xy;
    vec2 half_size = u_rect.zw * 0.5;
    float dist = sd_rounded_box(pixel - (u_rect.xy + half_size), half_size, u_radii);
    float cover = clamp(0.5 - dist, 0.0, 1.0);
    if (cover <= 0.0) {
        discard;
    }

    vec2 coord = (pixel - u_grab_origin) * u_inv_level;
    vec4 blurred = u_cubic > 0.5 ? fetch_cubic(coord) : fetch(coord);

    float luma = dot(blurred.rgb, vec3(0.2126, 0.7152, 0.0722));
    blurred.rgb = max(mix(vec3(luma), blurred.rgb, u_saturation), 0.0);
    blurred.rgb = min(blurred.rgb, blurred.a);

    vec4 color = u_tint + blurred * (1.0 - u_tint.a);
    float grain = (hash12(pixel) + hash12(pixel + vec2(17.0, 43.0)) - 1.0) * (u_noise / 255.0);
    color.rgb = clamp(color.rgb + grain * color.a, 0.0, color.a);
    gl_FragColor = color * (cover * u_opacity);
}
