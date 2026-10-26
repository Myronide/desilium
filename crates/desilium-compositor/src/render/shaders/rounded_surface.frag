//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif

#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

// Maps the sampled texture coordinate back to physical pixels relative to the
// clip rectangle (the window geometry), so every surface of a window is cut by
// the same rounded shape.
uniform vec2 u_tex_size;   // texture size in buffer pixels
uniform vec2 u_src_loc;    // sampled region origin (buffer px)
uniform vec2 u_src_size;   // sampled region size (buffer px)
uniform vec2 u_dst_size;   // element size on screen (physical px)
uniform vec2 u_dst_offset; // element origin minus clip origin (physical px)
uniform vec2 u_clip_size;  // clip rectangle size (physical px)
uniform vec4 u_radii;      // physical px: tl, tr, br, bl

float sd_rounded_box(vec2 p, vec2 half_size, vec4 radii) {
    float r = p.x > 0.0 ? (p.y > 0.0 ? radii.z : radii.y)
                        : (p.y > 0.0 ? radii.w : radii.x);
    r = min(r, min(half_size.x, half_size.y));
    vec2 q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void main() {
    vec4 color = texture2D(tex, v_coords);

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    vec2 local = (v_coords * u_tex_size - u_src_loc) / u_src_size * u_dst_size + u_dst_offset;
    float d = sd_rounded_box(local - u_clip_size * 0.5, u_clip_size * 0.5, u_radii);
    color *= clamp(0.5 - d, 0.0, 1.0) * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
