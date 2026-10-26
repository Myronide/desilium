// Full-area linear gradient with 2-4 stops and dithering.
uniform vec3 u_grad;   // direction xy (unit), z = gradient line length in px
uniform vec2 u_phys;
uniform vec4 u_c0;
uniform vec4 u_c1;
uniform vec4 u_c2;
uniform vec4 u_c3;
uniform vec4 u_pos;    // stop positions, ascending; unused stops are padded with 1.0
uniform float u_alpha;
uniform float u_dither;

float ramp(float t, float from, float to) {
    return clamp((t - from) / max(to - from, 1e-5), 0.0, 1.0);
}

void main() {
    vec2 p = v_coords * u_phys - u_phys * 0.5;
    float t = clamp(0.5 + dot(p, u_grad.xy) / max(u_grad.z, 1.0), 0.0, 1.0);

    // Sequential mixing: each stop takes over once t passes its predecessor.
    vec4 color = u_c0;
    color = mix(color, u_c1, ramp(t, u_pos.x, u_pos.y));
    color = mix(color, u_c2, ramp(t, u_pos.y, u_pos.z));
    color = mix(color, u_c3, ramp(t, u_pos.z, u_pos.w));

    if (u_dither > 0.0)
        color.rgb = clamp(color.rgb + dither_noise() * u_dither * color.a, 0.0, color.a);

    finish(color * u_alpha);
}
