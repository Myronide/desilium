// Soft drop shadow: analytic Gaussian-blurred rounded rectangle (single pass).
// The x direction is integrated in closed form with erf, the y direction is
// integrated numerically with a few samples, following the well known
// "fast blurred rounded rectangle" construction.
uniform vec2 u_phys;
uniform vec4 u_rect;     // shadow shape (after offset and spread), physical px
uniform vec4 u_radii;
uniform float u_sigma;   // physical px
uniform vec4 u_color;    // premultiplied
uniform vec4 u_knock;    // owning window rect; the shadow is cut out below it
uniform vec4 u_knock_radii;
uniform float u_knock_on;
uniform float u_alpha;

const int Y_SAMPLES = 8;
const float SIGMA_RANGE = 3.0;
const float SQRT_HALF = 0.70710678;
const float INV_SQRT_TAU = 0.39894228;

float gaussian(float x, float sigma) {
    return exp(-(x * x) / (2.0 * sigma * sigma)) * INV_SQRT_TAU / sigma;
}

float box_x(float x, float y, float sigma, float corner, vec2 half_size) {
    float delta = min(half_size.y - corner - abs(y), 0.0);
    float curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    vec2 integral = 0.5 + 0.5 * erf2((x + vec2(-curved, curved)) * (SQRT_HALF / sigma));
    return integral.y - integral.x;
}

float blurred_box(vec2 point, vec2 half_size, float sigma, float corner) {
    float low = point.y - half_size.y;
    float high = point.y + half_size.y;
    float start = clamp(-SIGMA_RANGE * sigma, low, high);
    float end = clamp(SIGMA_RANGE * sigma, low, high);
    float step = (end - start) / float(Y_SAMPLES);
    float y = start + step * 0.5;
    float value = 0.0;
    for (int i = 0; i < Y_SAMPLES; i++) {
        value += box_x(point.x, point.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step;
        y += step;
    }
    return value;
}

float quadrant_radius(vec2 p, vec4 radii) {
    return p.x > 0.0 ? (p.y > 0.0 ? radii.z : radii.y) : (p.y > 0.0 ? radii.w : radii.x);
}

void main() {
    vec2 pos = v_coords * u_phys;
    vec2 half_size = u_rect.zw * 0.5;
    vec2 p = pos - (u_rect.xy + half_size);
    float corner = min(quadrant_radius(p, u_radii), min(half_size.x, half_size.y));
    float level = clamp(blurred_box(p, half_size, u_sigma, corner), 0.0, 1.0);
    vec4 color = u_color * level;

    if (u_knock_on > 0.5) {
        vec2 knock_half = u_knock.zw * 0.5;
        vec2 kp = pos - (u_knock.xy + knock_half);
        color *= 1.0 - coverage(sd_rounded_box(kp, knock_half, u_knock_radii));
    }

    if (color.a > 0.0)
        color.rgb = clamp(color.rgb + dither_noise() * color.a, 0.0, color.a);

    finish(color * u_alpha);
}
