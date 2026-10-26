// Shared vertex shader of the backdrop blur passes. Positions arrive in clip space;
// every fragment shader derives its coordinates from gl_FragCoord, so no varyings.
attribute vec2 a_pos;

void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
