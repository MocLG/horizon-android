struct Player {
    direction: vec4<f32>,
};
@group(0) @binding(0) var<uniform> player: Player;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Out {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: Out;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

/// Whether (along, across) is inside a triangle pointing forward: its tip at
/// `tip`, its base at `base`, half as wide as `half` at the base.
fn in_triangle(along: f32, across: f32, tip: f32, base: f32, half: f32) -> bool {
    let t = (tip - along) / (tip - base);
    if (t < 0.0 || t > 1.0) {
        return false;
    }
    return abs(across) <= half * t;
}

/// A small Google Maps-style location marker: blue dot, white ring, soft
/// shadow, a blue direction arrow and a translucent heading cone.
@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    // `uv` is the map square; the centre is the player.
    let p = in.uv - vec2<f32>(0.5);
    var direction = player.direction.xy;
    let d = length(direction);
    if (d < 0.0001) {
        direction = vec2<f32>(0.0, 1.0);
    } else {
        direction = direction / d;
    }
    let forward = direction;
    let right = vec2<f32>(-forward.y, forward.x);
    let dist = length(p);
    let along = dot(p, forward);
    let across = dot(p, right);
    let aa = max(fwidth(dist), 0.0004);
    let blue = vec3<f32>(0.26, 0.52, 0.96);

    let arrow = in_triangle(along, across, 0.034, 0.015, 0.012);
    let arrow_outline = in_triangle(along, across, 0.037, 0.014, 0.014);
    let cone = along > 0.0 && dist < 0.050 && abs(across) <= along * 0.40;

    let dot_radius = 0.0055;
    let white_radius = 0.0075;
    let shadow_radius = 0.0095;
    let dot_cov = 1.0 - smoothstep(dot_radius - aa, dot_radius + aa, dist);
    let white_cov = 1.0 - smoothstep(white_radius - aa, white_radius + aa, dist);
    let shadow_cov = 1.0 - smoothstep(shadow_radius - aa, shadow_radius + aa, dist);

    if (!cone && !arrow_outline && shadow_cov <= 0.0) {
        discard;
    }

    // Back to front: cone, shadow, arrow, white ring, blue dot.
    var color = vec3<f32>(0.0, 0.0, 0.0);
    var alpha = 0.0;
    if (cone) {
        color = blue;
        alpha = 0.18 * (1.0 - dist / 0.050);
    }
    if (shadow_cov > 0.0) {
        color = vec3<f32>(0.0, 0.0, 0.0);
        alpha = max(alpha, shadow_cov * 0.35);
    }
    if (arrow_outline) {
        color = vec3<f32>(1.0, 1.0, 1.0);
        alpha = 1.0;
    }
    if (arrow) {
        color = blue;
        alpha = 1.0;
    }
    if (white_cov > 0.0) {
        color = vec3<f32>(1.0, 1.0, 1.0);
        alpha = max(alpha, white_cov);
    }
    if (dot_cov > 0.0) {
        color = blue;
        alpha = max(alpha, dot_cov);
    }
    return vec4<f32>(color, alpha);
}
