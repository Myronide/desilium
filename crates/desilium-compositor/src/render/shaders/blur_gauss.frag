// One direction of a separable Gaussian using the linear-sampling trick: two adjacent
// taps are merged into one bilinear fetch at their weighted centre.
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

#define PAIRS 8

uniform sampler2D tex;
uniform vec2 u_src_tex;
uniform vec2 u_src_content;
uniform vec2 u_dir;   // (1,0) or (0,1)
uniform float u_sigma; // in texels of this level

vec4 fetch(vec2 coord) {
    coord = clamp(coord, vec2(0.5), u_src_content - 0.5);
    return texture2D(tex, coord / u_src_tex);
}

void main() {
    vec2 c = gl_FragCoord.xy;
    float inv = -0.5 / (u_sigma * u_sigma);
    vec4 acc = fetch(c);
    float total = 1.0;
    for (int k = 0; k < PAIRS; k++) {
        float o1 = float(2 * k + 1);
        float o2 = o1 + 1.0;
        float w1 = exp(o1 * o1 * inv);
        float w2 = exp(o2 * o2 * inv);
        float w = w1 + w2;
        float off = (o1 * w1 + o2 * w2) / w;
        acc += w * (fetch(c + u_dir * off) + fetch(c - u_dir * off));
        total += 2.0 * w;
    }
    gl_FragColor = acc / total;
}
