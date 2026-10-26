// Dual-Kawase style 2x down-sample (4 bilinear corner taps + weighted centre).
// Each destination texel centre sits on the corner between four source texels, so the
// centre tap is an exact 2x2 box and the corner taps widen it to a smooth 4x4 footprint:
// no aliasing and no seams between texels.
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

uniform sampler2D tex;
uniform vec2 u_src_tex;     // allocated size of the source texture (texels)
uniform vec2 u_src_content; // valid region of the source texture (texels)

// Clamping to the valid region emulates clamp-to-edge inside a larger texture.
vec4 fetch(vec2 coord) {
    coord = clamp(coord, vec2(0.5), u_src_content - 0.5);
    return texture2D(tex, coord / u_src_tex);
}

void main() {
    vec2 c = gl_FragCoord.xy * 2.0;
    vec4 sum = fetch(c) * 4.0;
    sum += fetch(c + vec2(-1.0, -1.0));
    sum += fetch(c + vec2(1.0, -1.0));
    sum += fetch(c + vec2(-1.0, 1.0));
    sum += fetch(c + vec2(1.0, 1.0));
    gl_FragColor = sum * 0.125;
}
