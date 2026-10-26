// Filled rounded rectangle with optional gradient and inner/outer border.
uniform vec2 u_phys;      // element size in physical pixels
uniform vec4 u_rect;      // x, y, w, h of the shape inside the element (physical px)
uniform vec4 u_radii;     // physical px
uniform vec4 u_color_a;   // premultiplied, gradient start
uniform vec4 u_color_b;   // premultiplied, gradient end
uniform vec3 u_grad;      // direction xy (unit), z = gradient line length (px, 0 = solid)
uniform vec4 u_border;    // premultiplied
uniform float u_border_w; // physical px, 0 = none
uniform float u_border_outer;
uniform float u_alpha;
uniform float u_dither;

void main() {
    vec2 pos = v_coords * u_phys;
    vec2 half_size = u_rect.zw * 0.5;
    vec2 p = pos - (u_rect.xy + half_size);
    float d = sd_rounded_box(p, half_size, u_radii);
    float inside = coverage(d);

    float t = u_grad.z > 0.0 ? clamp(0.5 + dot(p, u_grad.xy) / u_grad.z, 0.0, 1.0) : 0.0;
    vec4 color = mix(u_color_a, u_color_b, t) * inside;

    if (u_border_w > 0.0) {
        float ring = u_border_outer > 0.5
            ? coverage(d - u_border_w) * (1.0 - inside)
            : inside * (1.0 - coverage(d + u_border_w));
        color = u_border * ring + color * (1.0 - u_border.a * ring);
    }

    if (u_dither > 0.0 && color.a > 0.0)
        color.rgb = clamp(color.rgb + dither_noise() * color.a, 0.0, color.a);

    finish(color * u_alpha);
}
