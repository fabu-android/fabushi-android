use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub const IDLE_ROTATION_DEG_PER_SEC: f64 = 4.5;
pub const SENDING_ROTATION_DEG_PER_SEC: f64 = 1.15;
pub const FOCUS_DURATION_MS: f64 = 2000.0;
pub const ZEN_ORBIT_CALM_RAD_PER_SEC: f64 = 0.12;
pub const ZEN_ORBIT_CIRCUMAMBULATE_RAD_PER_SEC: f64 = 0.55;

/// The 7 pulsing sacred lotus coordinates on the home globe (`home_world_2d_widget.dart`).
pub const GLOBE_LOTUS_NODES: &[(f64, f64)] = &[
    (34.0, 108.0),
    (23.0, 77.0),
    (1.0, 103.0),
    (-25.0, 133.0),
    (-33.0, -58.0),
    (-15.0, -47.0),
    (-32.0, 151.0),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorStop {
    pub offset: f64,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearGradient {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadialGradient {
    pub fx: f64,
    pub fy: f64,
    pub r0: f64,
    pub cx: f64,
    pub cy: f64,
    pub r1: f64,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PathOp {
    MoveTo { x: f64, y: f64 },
    LineTo { x: f64, y: f64 },
    QuadraticTo { cx: f64, cy: f64, x: f64, y: f64 },
    CubicTo {
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    },
    Close,
}

/// Declarative Canvas2D draw command emitted by pure-Rust scene projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum CanvasDrawCmd {
    DrawBackgroundImage {
        asset: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        alpha: f64,
    },
    DrawGlobeTextureSphere {
        cx: f64,
        cy: f64,
        radius: f64,
        rotation_lon_deg: f64,
        tilt_lat_deg: f64,
        asset: String,
        is_3d: bool,
    },
    DrawLinearRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        gradient: Option<LinearGradient>,
        fill: Option<String>,
        corner_radius: f64,
        alpha: f64,
    },
    DrawRadialCircle {
        cx: f64,
        cy: f64,
        r: f64,
        gradient: RadialGradient,
        blur: f64,
        alpha: f64,
    },
    DrawCircle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Option<String>,
        stroke: Option<String>,
        line_width: f64,
        blur: f64,
        alpha: f64,
    },
    DrawEllipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        rotation: f64,
        fill: Option<String>,
        stroke: Option<String>,
        line_width: f64,
        blur: f64,
        alpha: f64,
    },
    DrawLine {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        stroke: Option<String>,
        gradient: Option<LinearGradient>,
        line_width: f64,
        blur: f64,
        alpha: f64,
        line_cap_round: bool,
    },
    DrawPath {
        ops: Vec<PathOp>,
        fill: Option<String>,
        fill_linear: Option<LinearGradient>,
        fill_radial: Option<RadialGradient>,
        stroke: Option<String>,
        line_width: f64,
        blur: f64,
        alpha: f64,
        line_cap_round: bool,
    },
    DrawPolygon {
        points: Vec<[f64; 2]>,
        fill: String,
        stroke: Option<String>,
        line_width: f64,
        alpha: f64,
    },
    DrawText {
        text: String,
        x: f64,
        y: f64,
        color: String,
        font_size: f64,
        bold: bool,
        align: String,
        shadow_color: Option<String>,
        shadow_blur: f64,
        alpha: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectedPoint {
    pub x: f64,
    pub y: f64,
    /// Depth component in `[-1.0, 1.0]`: `+1.0` faces directly toward camera.
    pub z: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferBeam {
    pub from_lat: f64,
    pub from_lng: f64,
    pub to_lat: f64,
    pub to_lng: f64,
    pub color: String,
    pub duration_ms: f64,
    pub started_at_ms: f64,
    pub to_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobeSceneState {
    pub time_seconds: f64,
    pub rotation_lon_deg: f64,
    pub tilt_lat_deg: f64,
    pub focus_start_lon_deg: Option<f64>,
    pub focus_target_lon_deg: Option<f64>,
    pub focus_start_ms: f64,
    pub beams: Vec<TransferBeam>,
    pub is_dragging: bool,
}

impl Default for GlobeSceneState {
    fn default() -> Self {
        Self {
            time_seconds: 0.0,
            rotation_lon_deg: 108.0,
            tilt_lat_deg: 0.0,
            focus_start_lon_deg: None,
            focus_target_lon_deg: None,
            focus_start_ms: 0.0,
            beams: Vec::new(),
            is_dragging: false,
        }
    }
}

impl GlobeSceneState {
    pub fn focus_on_longitude(&mut self, target_lon_deg: f64, now_ms: f64) {
        self.focus_start_lon_deg = Some(self.rotation_lon_deg);
        self.focus_target_lon_deg = Some(normalize_longitude(target_lon_deg));
        self.focus_start_ms = now_ms;
    }

    pub fn add_beam(
        &mut self,
        from_lat: f64,
        from_lng: f64,
        to_lat: f64,
        to_lng: f64,
        to_label: Option<String>,
        now_ms: f64,
        duration_ms: f64,
    ) {
        self.focus_on_longitude(to_lng, now_ms);
        self.beams.push(TransferBeam {
            from_lat,
            from_lng,
            to_lat,
            to_lng,
            color: "#FFD76B".to_string(),
            duration_ms: duration_ms.max(400.0),
            started_at_ms: now_ms,
            to_label,
        });
        if self.beams.len() > 8 {
            let excess = self.beams.len() - 8;
            self.beams.drain(0..excess);
        }
    }

    pub fn clear_beams(&mut self) {
        self.beams.clear();
        self.focus_start_lon_deg = None;
        self.focus_target_lon_deg = None;
    }

    pub fn drag(&mut self, dx: f64, dy: f64, is_3d: bool) {
        self.focus_start_lon_deg = None;
        self.focus_target_lon_deg = None;
        self.rotation_lon_deg = normalize_longitude(self.rotation_lon_deg - dx * 0.35);
        if is_3d {
            self.tilt_lat_deg = (self.tilt_lat_deg + dy * 0.25).clamp(-55.0, 55.0);
        }
    }

    /// Advances globe rotation, focus cubic ease-out, and beam lifecycles.
    pub fn advance(&mut self, dt_ms: f64, now_ms: f64) {
        let dt_sec = (dt_ms / 1000.0).clamp(0.0, 1.0);
        self.time_seconds += dt_sec;

        self.beams.retain(|beam| {
            let age_ms = now_ms - beam.started_at_ms;
            age_ms <= beam.duration_ms + 1200.0
        });

        if self.is_dragging {
            return;
        }

        if let (Some(start), Some(target)) = (self.focus_start_lon_deg, self.focus_target_lon_deg) {
            let progress = ((now_ms - self.focus_start_ms) / FOCUS_DURATION_MS).clamp(0.0, 1.0);
            let eased = cubic_ease_out(progress);
            let delta = shortest_longitude_delta(start, target);
            self.rotation_lon_deg = normalize_longitude(start + delta * eased);
            if progress >= 1.0 {
                self.focus_start_lon_deg = None;
                self.focus_target_lon_deg = None;
            }
        } else {
            let speed = if self.beams.is_empty() {
                IDLE_ROTATION_DEG_PER_SEC
            } else {
                SENDING_ROTATION_DEG_PER_SEC
            };
            self.rotation_lon_deg = normalize_longitude(self.rotation_lon_deg + dt_sec * speed);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZenSceneState {
    pub time_seconds: f64,
    pub auto_rotate: bool,
    pub is_burning: bool,
    pub incense_progress: f64,
    pub buddha_rot_y: f64,
    pub buddha_rot_x: f64,
    pub is_dragging: bool,
}

impl Default for ZenSceneState {
    fn default() -> Self {
        Self {
            time_seconds: 0.0,
            auto_rotate: false,
            is_burning: false,
            incense_progress: 0.0,
            buddha_rot_y: 0.0,
            buddha_rot_x: 0.14,
            is_dragging: false,
        }
    }
}

impl ZenSceneState {
    pub fn drag(&mut self, dx: f64, dy: f64) {
        self.buddha_rot_y += dx * 0.01;
        self.buddha_rot_x = (self.buddha_rot_x + dy * 0.008).clamp(-0.55, 0.55);
    }

    pub fn advance(&mut self, dt_ms: f64) {
        let dt_sec = (dt_ms / 1000.0).clamp(0.0, 1.0);
        self.time_seconds += dt_sec;

        if !self.is_dragging {
            if self.auto_rotate {
                self.buddha_rot_y += 0.5 * dt_sec;
            } else if self.buddha_rot_y.abs() > 0.001 {
                // Smoothly return toward front-facing center when circumambulation pauses.
                let two_pi = 2.0 * PI;
                let mut norm = self.buddha_rot_y.rem_euclid(two_pi);
                if norm > PI {
                    norm -= two_pi;
                }
                if norm.abs() < 0.04 {
                    self.buddha_rot_y = 0.0;
                } else {
                    self.buddha_rot_y -= norm * (dt_sec * 2.4).min(1.0);
                }
            }
        }
    }
}

/// Normalizes any longitude angle to `[-180, 180)`.
pub fn normalize_longitude(lng: f64) -> f64 {
    ((lng + 540.0).rem_euclid(360.0)) - 180.0
}

/// Computes the signed shortest angular delta from `from` to `to` in `[-180, 180)`.
pub fn shortest_longitude_delta(from: f64, to: f64) -> f64 {
    ((to - from + 540.0).rem_euclid(360.0)) - 180.0
}

/// Cubic ease-out `1 - (1 - t)^3`, clamped to `[0, 1]`.
pub fn cubic_ease_out(t: f64) -> f64 {
    let clamped = t.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}

/// Projects geographic `(lat_deg, lng_deg)` onto screen space with optional 3D tilt.
/// Returns `None` when behind the visible horizon (`z < min_visible_z`).
pub fn project_lat_lng_3d(
    lat_deg: f64,
    lng_deg: f64,
    center_x: f64,
    center_y: f64,
    radius: f64,
    rotation_lon_deg: f64,
    tilt_lat_deg: f64,
    min_visible_z: f64,
) -> Option<ProjectedPoint> {
    let rel_lng = normalize_longitude(lng_deg - rotation_lon_deg).to_radians();
    let lat = lat_deg.to_radians();
    let tilt = tilt_lat_deg.to_radians();

    let x0 = lat.cos() * rel_lng.sin();
    let y0 = lat.sin();
    let z0 = lat.cos() * rel_lng.cos();

    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    let y1 = y0 * cos_t - z0 * sin_t;
    let z1 = y0 * sin_t + z0 * cos_t;

    if z1 < min_visible_z {
        return None;
    }

    Some(ProjectedPoint {
        x: center_x + radius * x0,
        y: center_y - radius * y1,
        z: z1,
    })
}

fn quadratic_point(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64, t: f64) -> (f64, f64) {
    let omt = 1.0 - t;
    (
        omt * omt * ax + 2.0 * omt * t * bx + t * t * cx,
        omt * omt * ay + 2.0 * omt * t * by + t * t * cy,
    )
}

fn push_lotus_mark(cmds: &mut Vec<CanvasDrawCmd>, cx: f64, cy: f64, radius: f64) {
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy,
        r: radius * 2.6,
        fill: Some("rgba(255, 215, 107, 0.33)".to_string()),
        stroke: None,
        line_width: 0.0,
        blur: 8.0,
        alpha: 1.0,
    });

    for &angle in &[-0.75_f64, -0.38, 0.0, 0.38, 0.75] {
        let rot = |px: f64, py: f64| -> (f64, f64) {
            let c = angle.cos();
            let s = angle.sin();
            (cx + px * c - py * s, cy + px * s + py * c)
        };
        let (p0x, p0y) = rot(0.0, radius);
        let (c1x, c1y) = rot(-radius * 0.82, 0.0);
        let (topx, topy) = rot(0.0, -radius * 1.45);
        let (c2x, c2y) = rot(radius * 0.82, 0.0);
        let (g0x, g0y) = rot(0.0, -radius * 1.5);
        let (g1x, g1y) = rot(0.0, radius);

        cmds.push(CanvasDrawCmd::DrawPath {
            ops: vec![
                PathOp::MoveTo { x: p0x, y: p0y },
                PathOp::QuadraticTo {
                    cx: c1x,
                    cy: c1y,
                    x: topx,
                    y: topy,
                },
                PathOp::QuadraticTo {
                    cx: c2x,
                    cy: c2y,
                    x: p0x,
                    y: p0y,
                },
                PathOp::Close,
            ],
            fill: None,
            fill_linear: Some(LinearGradient {
                x0: g0x,
                y0: g0y,
                x1: g1x,
                y1: g1y,
                stops: vec![
                    ColorStop {
                        offset: 0.0,
                        color: "#FFFFFF".to_string(),
                    },
                    ColorStop {
                        offset: 1.0,
                        color: "#FFD43B".to_string(),
                    },
                ],
            }),
            fill_radial: None,
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: false,
        });
    }
}

/// Builds 3D wireframe latitude/longitude graticule curves projected onto the globe.
pub fn build_3d_wireframe_graticule(
    cmds: &mut Vec<CanvasDrawCmd>,
    cx: f64,
    cy: f64,
    radius: f64,
    rotation_lon_deg: f64,
    tilt_lat_deg: f64,
) {
    // Latitude parallels
    for &lat in &[-60.0_f64, -30.0, 0.0, 30.0, 60.0] {
        let mut ops: Vec<PathOp> = Vec::new();
        let mut pen_down = false;
        for step in (0..=360).step_by(10) {
            let lng = step as f64 - 180.0;
            if let Some(pt) = project_lat_lng_3d(
                lat,
                lng,
                cx,
                cy,
                radius,
                rotation_lon_deg,
                tilt_lat_deg,
                0.0,
            ) {
                if !pen_down {
                    ops.push(PathOp::MoveTo { x: pt.x, y: pt.y });
                    pen_down = true;
                } else {
                    ops.push(PathOp::LineTo { x: pt.x, y: pt.y });
                }
            } else {
                pen_down = false;
            }
        }
        if !ops.is_empty() {
            cmds.push(CanvasDrawCmd::DrawPath {
                ops,
                fill: None,
                fill_linear: None,
                fill_radial: None,
                stroke: Some("rgba(255, 215, 107, 0.24)".to_string()),
                line_width: if lat == 0.0 { 1.1 } else { 0.7 },
                blur: 0.0,
                alpha: 1.0,
                line_cap_round: true,
            });
        }
    }

    // Longitude meridians
    for lon_step in (0..360).step_by(30) {
        let lng = lon_step as f64 - 180.0;
        let mut ops: Vec<PathOp> = Vec::new();
        let mut pen_down = false;
        for lat_step in (-90..=90).step_by(10) {
            let lat = lat_step as f64;
            if let Some(pt) = project_lat_lng_3d(
                lat,
                lng,
                cx,
                cy,
                radius,
                rotation_lon_deg,
                tilt_lat_deg,
                0.0,
            ) {
                if !pen_down {
                    ops.push(PathOp::MoveTo { x: pt.x, y: pt.y });
                    pen_down = true;
                } else {
                    ops.push(PathOp::LineTo { x: pt.x, y: pt.y });
                }
            } else {
                pen_down = false;
            }
        }
        if !ops.is_empty() {
            cmds.push(CanvasDrawCmd::DrawPath {
                ops,
                fill: None,
                fill_linear: None,
                fill_radial: None,
                stroke: Some("rgba(255, 255, 255, 0.22)".to_string()),
                line_width: 0.7,
                blur: 0.0,
                alpha: 1.0,
                line_cap_round: true,
            });
        }
    }
}

/// Renders the 2D/3D Global Dharma Transmission scene into declarative `CanvasDrawCmd`s.
pub fn render_home_scene(
    state: &GlobeSceneState,
    width: f64,
    height: f64,
    now_ms: f64,
    is_3d: bool,
) -> Vec<CanvasDrawCmd> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let mut cmds = Vec::with_capacity(96);
    let t = state.time_seconds;
    let cx = width / 2.0;
    let cy = height * 0.34;
    let radius = (width.min(height) * 0.34).clamp(122.0, 260.0);

    // 1. Cosmic deep space + lightfield backdrop
    cmds.push(CanvasDrawCmd::DrawLinearRect {
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
        gradient: Some(LinearGradient {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "#0B0E14".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "#1A237E".to_string(),
                },
            ],
        }),
        fill: None,
        corner_radius: 0.0,
        alpha: 1.0,
    });
    cmds.push(CanvasDrawCmd::DrawBackgroundImage {
        asset: "home_world_lightfield.webp".to_string(),
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
        alpha: 0.92,
    });

    // 2. Pulsing atmosphere halo + 18 floating star motes
    let pulse = 0.5 + (t * 1.2).sin() * 0.5;
    let halo_alpha = 0.33 + pulse * 0.20;
    cmds.push(CanvasDrawCmd::DrawRadialCircle {
        cx,
        cy,
        r: width.max(height) * 0.65,
        gradient: RadialGradient {
            fx: cx,
            fy: cy,
            r0: 0.0,
            cx,
            cy,
            r1: width.max(height) * 0.65,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: format!("rgba(255, 238, 185, {halo_alpha:.3})"),
                },
                ColorStop {
                    offset: 1.0,
                    color: "rgba(0, 0, 0, 0.0)".to_string(),
                },
            ],
        },
        blur: 0.0,
        alpha: 1.0,
    });

    for i in 0..18 {
        let phase = t * 0.08 + (i as f64) * 0.37;
        let sx = ((phase * 1.7).sin() * 0.5 + 0.5) * width;
        let sy = ((phase * 1.1).cos() * 0.5 + 0.5) * height * 0.58;
        let opacity = 0.12 + ((phase * 2.3).sin() * 0.5 + 0.5) * 0.18;
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: sx,
            cy: sy,
            r: 1.2 + (i % 4) as f64 * 0.8,
            fill: Some(format!("rgba(255, 246, 196, {opacity:.3})")),
            stroke: None,
            line_width: 0.0,
            blur: 2.0,
            alpha: 1.0,
        });
    }

    // 3. Outer sphere glow + earth texture sphere
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy,
        r: radius * 1.08,
        fill: Some("rgba(255, 255, 255, 0.26)".to_string()),
        stroke: None,
        line_width: 0.0,
        blur: 18.0,
        alpha: 1.0,
    });
    let tilt = if is_3d { state.tilt_lat_deg } else { 0.0 };
    cmds.push(CanvasDrawCmd::DrawGlobeTextureSphere {
        cx,
        cy,
        radius,
        rotation_lon_deg: state.rotation_lon_deg,
        tilt_lat_deg: tilt,
        asset: "earth_texture.jpg".to_string(),
        is_3d,
    });

    // Rim ring
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy,
        r: radius,
        fill: None,
        stroke: Some(if is_3d {
            "rgba(255, 215, 0, 0.88)".to_string()
        } else {
            "rgba(255, 242, 168, 0.80)".to_string()
        }),
        line_width: if is_3d { 1.6 } else { 1.2 },
        blur: 1.5,
        alpha: 1.0,
    });

    // 4. Graticule wireframe (full 3D spherical projection in 3D mode, classic ellipses+curves in 2D)
    if is_3d {
        build_3d_wireframe_graticule(&mut cmds, cx, cy, radius, state.rotation_lon_deg, tilt);
    } else {
        for &scale_y in &[0.22_f64, 0.42, 0.62] {
            cmds.push(CanvasDrawCmd::DrawEllipse {
                cx,
                cy,
                rx: radius * 0.86,
                ry: radius * scale_y,
                rotation: 0.0,
                fill: None,
                stroke: Some("rgba(255, 255, 255, 0.20)".to_string()),
                line_width: 0.7,
                blur: 0.0,
                alpha: 1.0,
            });
        }
        for &offset in &[-0.46_f64, -0.22, 0.0, 0.22, 0.46] {
            cmds.push(CanvasDrawCmd::DrawPath {
                ops: vec![
                    PathOp::MoveTo {
                        x: cx + radius * offset,
                        y: cy - radius * 0.96,
                    },
                    PathOp::CubicTo {
                        c1x: cx + radius * offset * 1.9,
                        c1y: cy - radius * 0.42,
                        c2x: cx + radius * offset * 1.9,
                        c2y: cy + radius * 0.42,
                        x: cx + radius * offset,
                        y: cy + radius * 0.96,
                    },
                ],
                fill: None,
                fill_linear: None,
                fill_radial: None,
                stroke: Some("rgba(255, 255, 255, 0.20)".to_string()),
                line_width: 0.7,
                blur: 0.0,
                alpha: 1.0,
                line_cap_round: false,
            });
        }
    }

    // 5. 7 pulsing sacred lotus nodes
    for (i, &(lat, lng)) in GLOBE_LOTUS_NODES.iter().enumerate() {
        if let Some(pos) = project_lat_lng_3d(
            lat,
            lng,
            cx,
            cy,
            radius,
            state.rotation_lon_deg,
            tilt,
            -0.16,
        ) {
            let node_pulse = 0.5 + (t * 1.5 + i as f64).sin() * 0.5;
            push_lotus_mark(&mut cmds, pos.x, pos.y, 8.0 + node_pulse * 2.4);
        }
    }

    // 6. Quadratic Bezier golden Dharma beams (#FFD76B) + comet tails
    for beam in &state.beams {
        let age_ms = (now_ms - beam.started_at_ms).max(0.0);
        let progress = (age_ms / beam.duration_ms).clamp(0.0, 1.0);
        if progress <= 0.0 {
            continue;
        }
        let start = match project_lat_lng_3d(
            beam.from_lat,
            beam.from_lng,
            cx,
            cy,
            radius,
            state.rotation_lon_deg,
            tilt,
            -0.16,
        ) {
            Some(p) => p,
            None => continue,
        };
        let end = match project_lat_lng_3d(
            beam.to_lat,
            beam.to_lng,
            cx,
            cy,
            radius,
            state.rotation_lon_deg,
            tilt,
            -0.16,
        ) {
            Some(p) => p,
            None => continue,
        };
        let ctrl_x = (start.x + end.x) / 2.0;
        let ctrl_y = start.y.min(end.y) - radius * (0.24 + progress * 0.10);

        cmds.push(CanvasDrawCmd::DrawPath {
            ops: vec![
                PathOp::MoveTo {
                    x: start.x,
                    y: start.y,
                },
                PathOp::QuadraticTo {
                    cx: ctrl_x,
                    cy: ctrl_y,
                    x: end.x,
                    y: end.y,
                },
            ],
            fill: None,
            fill_linear: None,
            fill_radial: None,
            stroke: Some("rgba(255, 215, 107, 0.25)".to_string()),
            line_width: 1.1,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: true,
        });

        let (hx, hy) = quadratic_point(start.x, start.y, ctrl_x, ctrl_y, end.x, end.y, progress);
        let tail_t = (progress - 0.18).clamp(0.0, 1.0);
        let (tx, ty) = quadratic_point(start.x, start.y, ctrl_x, ctrl_y, end.x, end.y, tail_t);

        cmds.push(CanvasDrawCmd::DrawLine {
            x0: tx,
            y0: ty,
            x1: hx,
            y1: hy,
            stroke: None,
            gradient: Some(LinearGradient {
                x0: tx,
                y0: ty,
                x1: hx,
                y1: hy,
                stops: vec![
                    ColorStop {
                        offset: 0.0,
                        color: "rgba(255, 215, 107, 0.0)".to_string(),
                    },
                    ColorStop {
                        offset: 0.65,
                        color: "rgba(255, 215, 107, 0.85)".to_string(),
                    },
                    ColorStop {
                        offset: 1.0,
                        color: "#FFFFFF".to_string(),
                    },
                ],
            }),
            line_width: 3.0,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: true,
        });

        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: hx,
            cy: hy,
            r: 4.6,
            fill: Some("#FFFFFF".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 4.0,
            alpha: 1.0,
        });
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: hx,
            cy: hy,
            r: 2.0,
            fill: Some("#FFF4B8".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 1.0,
        });

        if progress > 0.82 {
            if let Some(label) = &beam.to_label {
                cmds.push(CanvasDrawCmd::DrawText {
                    text: label.clone(),
                    x: end.x,
                    y: end.y + 16.0,
                    color: "#FFFFFF".to_string(),
                    font_size: 12.0,
                    bold: true,
                    align: "center".to_string(),
                    shadow_color: Some("rgba(0,0,0,0.65)".to_string()),
                    shadow_blur: 6.0,
                    alpha: 1.0,
                });
            }
        }
    }

    // 7. Bottom warm glow
    cmds.push(CanvasDrawCmd::DrawLinearRect {
        x: 0.0,
        y: height * 0.62,
        w: width,
        h: height * 0.38,
        gradient: Some(LinearGradient {
            x0: 0.0,
            y0: height * 0.62,
            x1: 0.0,
            y1: height,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "rgba(0, 0, 0, 0.0)".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "rgba(255, 246, 212, 0.36)".to_string(),
                },
            ],
        }),
        fill: None,
        corner_radius: 0.0,
        alpha: 1.0,
    });

    cmds
}

/// Computes breathing scale offset `sin(t * pi * 0.42) * 0.012`.
pub fn zen_breathing_offset(time_seconds: f64) -> f64 {
    (time_seconds * PI * 0.42).sin() * 0.012
}

/// Computes vertical floating offset `sin(t * pi * 0.34) * 4.0`.
pub fn zen_float_offset(time_seconds: f64) -> f64 {
    (time_seconds * PI * 0.34).sin() * 4.0
}

fn push_2d_buddha(
    cmds: &mut Vec<CanvasDrawCmd>,
    cx: f64,
    cy: f64,
    scale: f64,
) {
    // Lotus seat pedestal behind knees
    for i in 0..9 {
        let angle = -0.8 + i as f64 * 0.2;
        let px = cx + (angle.sin()) * 64.0 * scale;
        let py = cy + 92.0 * scale + (angle.cos().abs()) * 10.0 * scale;
        cmds.push(CanvasDrawCmd::DrawEllipse {
            cx: px,
            cy: py,
            rx: 15.0 * scale,
            ry: 24.0 * scale,
            rotation: angle * 0.55,
            fill: Some(if i % 2 == 0 {
                "rgba(255, 209, 224, 0.90)".to_string()
            } else {
                "rgba(229, 134, 168, 0.88)".to_string()
            }),
            stroke: Some("rgba(255, 215, 107, 0.65)".to_string()),
            line_width: 1.0,
            blur: 0.0,
            alpha: 1.0,
        });
    }

    // Robed meditating silhouette
    cmds.push(CanvasDrawCmd::DrawPath {
        ops: vec![
            PathOp::MoveTo {
                x: cx,
                y: cy - 82.0 * scale,
            },
            PathOp::CubicTo {
                c1x: cx + 62.0 * scale,
                c1y: cy - 64.0 * scale,
                c2x: cx + 82.0 * scale,
                c2y: cy + 28.0 * scale,
                x: cx + 74.0 * scale,
                y: cy + 88.0 * scale,
            },
            PathOp::LineTo {
                x: cx - 74.0 * scale,
                y: cy + 88.0 * scale,
            },
            PathOp::CubicTo {
                c1x: cx - 82.0 * scale,
                c1y: cy + 28.0 * scale,
                c2x: cx - 62.0 * scale,
                c2y: cy - 64.0 * scale,
                x: cx,
                y: cy - 82.0 * scale,
            },
            PathOp::Close,
        ],
        fill: None,
        fill_linear: Some(LinearGradient {
            x0: cx - 60.0 * scale,
            y0: cy - 80.0 * scale,
            x1: cx + 65.0 * scale,
            y1: cy + 90.0 * scale,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "#FFF2BD".to_string(),
                },
                ColorStop {
                    offset: 0.45,
                    color: "#FFD700".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "#9A6814".to_string(),
                },
            ],
        }),
        fill_radial: None,
        stroke: Some("rgba(255, 246, 196, 0.72)".to_string()),
        line_width: 1.5,
        blur: 0.0,
        alpha: 1.0,
        line_cap_round: true,
    });

    // Robe draped fold arcs
    for row in 0..4 {
        let fy = cy - 28.0 * scale + row as f64 * 24.0 * scale;
        let span = (44.0 + row as f64 * 8.0) * scale;
        cmds.push(CanvasDrawCmd::DrawPath {
            ops: vec![
                PathOp::MoveTo {
                    x: cx - span,
                    y: fy,
                },
                PathOp::QuadraticTo {
                    cx,
                    cy: fy + 16.0 * scale,
                    x: cx + span,
                    y: fy,
                },
            ],
            fill: None,
            fill_linear: None,
            fill_radial: None,
            stroke: Some("rgba(122, 75, 11, 0.48)".to_string()),
            line_width: 1.8 * scale,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: true,
        });
    }

    // Dhyana mudra hands & luminous pearl
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx,
        cy: cy + 44.0 * scale,
        rx: 22.0 * scale,
        ry: 10.0 * scale,
        rotation: 0.0,
        fill: Some("#FFE49E".to_string()),
        stroke: Some("#9A6814".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
    });
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy: cy + 38.0 * scale,
        r: 7.5 * scale,
        fill: Some("#FFF8D4".to_string()),
        stroke: Some("#FFD700".to_string()),
        line_width: 1.0,
        blur: 4.0,
        alpha: 1.0,
    });

    // Head + Ushnisha + Urna
    let head_y = cy - 98.0 * scale;
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy: head_y - 36.0 * scale,
        r: 14.0 * scale,
        fill: Some("#D4A12A".to_string()),
        stroke: Some("#FFF0B8".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
    });
    // Ears
    for &sign in &[-1.0_f64, 1.0] {
        cmds.push(CanvasDrawCmd::DrawEllipse {
            cx: cx + sign * 35.0 * scale,
            cy: head_y + 4.0 * scale,
            rx: 5.5 * scale,
            ry: 15.0 * scale,
            rotation: 0.0,
            fill: Some("#E2B13B".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 1.0,
        });
    }
    cmds.push(CanvasDrawCmd::DrawPath {
        ops: vec![
            PathOp::MoveTo {
                x: cx - 35.0 * scale,
                y: head_y,
            },
            PathOp::CubicTo {
                c1x: cx - 35.0 * scale,
                c1y: head_y - 46.0 * scale,
                c2x: cx + 35.0 * scale,
                c2y: head_y - 46.0 * scale,
                x: cx + 35.0 * scale,
                y: head_y,
            },
            PathOp::CubicTo {
                c1x: cx + 35.0 * scale,
                c1y: head_y + 46.0 * scale,
                c2x: cx - 35.0 * scale,
                c2y: head_y + 46.0 * scale,
                x: cx - 35.0 * scale,
                y: head_y,
            },
            PathOp::Close,
        ],
        fill: None,
        fill_linear: None,
        fill_radial: Some(RadialGradient {
            fx: cx - 10.0 * scale,
            fy: head_y - 12.0 * scale,
            r0: 2.0,
            cx,
            cy: head_y,
            r1: 42.0 * scale,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "#FFF7CC".to_string(),
                },
                ColorStop {
                    offset: 0.55,
                    color: "#FFD700".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "#A46D12".to_string(),
                },
            ],
        }),
        stroke: Some("rgba(255, 240, 184, 0.7)".to_string()),
        line_width: 1.2,
        blur: 0.0,
        alpha: 1.0,
        line_cap_round: false,
    });
    // Serene closed eyes & urna
    for &sign in &[-1.0_f64, 1.0] {
        cmds.push(CanvasDrawCmd::DrawPath {
            ops: vec![
                PathOp::MoveTo {
                    x: cx + sign * 8.0 * scale,
                    y: head_y - 2.0 * scale,
                },
                PathOp::QuadraticTo {
                    cx: cx + sign * 14.0 * scale,
                    cy: head_y + 2.5 * scale,
                    x: cx + sign * 20.0 * scale,
                    y: head_y - 2.0 * scale,
                },
            ],
            fill: None,
            fill_linear: None,
            fill_radial: None,
            stroke: Some("#6B4209".to_string()),
            line_width: 1.6 * scale,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: true,
        });
    }
    cmds.push(CanvasDrawCmd::DrawCircle {
        cx,
        cy: head_y - 12.0 * scale,
        r: 2.4 * scale,
        fill: Some("#FFFDF0".to_string()),
        stroke: None,
        line_width: 0.0,
        blur: 2.0,
        alpha: 1.0,
    });
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn sub(self, o: Self) -> Self {
        Self {
            x: self.x - o.x,
            y: self.y - o.y,
            z: self.z - o.z,
        }
    }
    fn cross(self, o: Self) -> Self {
        Self {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }
    fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn norm(self) -> Self {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if len < 1e-9 {
            Self {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            }
        } else {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        }
    }
}

struct MeshQuad {
    verts: [Vec3; 4],
    base_rgb: [f64; 3],
    metallic: f64,
}

fn push_lathe_rings(
    quads: &mut Vec<MeshQuad>,
    rings: &[(f64, f64, f64)], // (y, rx, rz)
    segments: usize,
    base_rgb: [f64; 3],
    metallic: f64,
) {
    for r in 0..rings.len().saturating_sub(1) {
        let (y0, rx0, rz0) = rings[r];
        let (y1, rx1, rz1) = rings[r + 1];
        for s in 0..segments {
            let a0 = (s as f64 / segments as f64) * 2.0 * PI;
            let a1 = ((s + 1) as f64 / segments as f64) * 2.0 * PI;
            let v00 = Vec3 {
                x: rx0 * a0.cos(),
                y: y0,
                z: rz0 * a0.sin(),
            };
            let v10 = Vec3 {
                x: rx0 * a1.cos(),
                y: y0,
                z: rz0 * a1.sin(),
            };
            let v11 = Vec3 {
                x: rx1 * a1.cos(),
                y: y1,
                z: rz1 * a1.sin(),
            };
            let v01 = Vec3 {
                x: rx1 * a0.cos(),
                y: y1,
                z: rz1 * a0.sin(),
            };
            quads.push(MeshQuad {
                verts: [v00, v10, v11, v01],
                base_rgb,
                metallic,
            });
        }
    }
}

/// Projects the 3D golden lotus-throne Buddha mesh matching `buddha_3d.html` aesthetics.
pub fn project_3d_buddha_mesh(
    cmds: &mut Vec<CanvasDrawCmd>,
    cx: f64,
    cy: f64,
    scale_px: f64,
    rot_y: f64,
    rot_x: f64,
    breath_scale: f64,
) {
    let mut quads: Vec<MeshQuad> = Vec::with_capacity(240);
    let gold = [255.0, 215.0, 48.0];
    let deep_gold = [212.0, 165.0, 38.0];
    let pink_lotus = [238.0, 145.0, 180.0];

    // 1. Lotus Throne Pedestal
    push_lathe_rings(
        &mut quads,
        &[
            (-1.26, 0.96, 0.72),
            (-1.12, 1.04, 0.78),
            (-0.96, 0.74, 0.56),
            (-0.78, 0.92, 0.68),
            (-0.62, 0.88, 0.64),
        ],
        14,
        deep_gold,
        0.75,
    );

    // 2. Sculpted 12-petal Lotus Throne ring
    for i in 0..12 {
        let a = (i as f64 / 12.0) * 2.0 * PI;
        let da = 0.22;
        let r_in = 0.72;
        let r_mid = 1.08;
        let r_tip = 0.96;
        let z_scale = 0.75;
        let v0 = Vec3 {
            x: r_in * (a - da).cos(),
            y: -0.90,
            z: r_in * z_scale * (a - da).sin(),
        };
        let v1 = Vec3 {
            x: r_in * (a + da).cos(),
            y: -0.90,
            z: r_in * z_scale * (a + da).sin(),
        };
        let v2 = Vec3 {
            x: r_mid * (a + da * 0.55).cos(),
            y: -0.70,
            z: r_mid * z_scale * (a + da * 0.55).sin(),
        };
        let v3 = Vec3 {
            x: r_tip * a.cos(),
            y: -0.60,
            z: r_tip * z_scale * a.sin(),
        };
        quads.push(MeshQuad {
            verts: [v0, v1, v2, v3],
            base_rgb: pink_lotus,
            metallic: 0.35,
        });
    }

    // 3. Crossed Padmasana legs + robe torso + neck
    push_lathe_rings(
        &mut quads,
        &[
            (-0.62, 0.84, 0.54),
            (-0.42, 0.78, 0.48),
            (-0.22, 0.52, 0.36),
            (0.06, 0.44, 0.30),
            (0.32, 0.50, 0.30),
            (0.46, 0.22, 0.20),
            (0.56, 0.16, 0.16),
        ],
        12,
        gold,
        0.9,
    );

    // 4. Head sphere + Ushnisha crown
    push_lathe_rings(
        &mut quads,
        &[
            (0.56, 0.18, 0.18),
            (0.68, 0.29, 0.28),
            (0.84, 0.31, 0.30),
            (0.98, 0.24, 0.24),
            (1.06, 0.12, 0.12),
            (1.16, 0.14, 0.14),
            (1.24, 0.04, 0.04),
        ],
        12,
        gold,
        0.92,
    );

    // Transform & project
    let cy_rot = rot_y.cos();
    let sy_rot = rot_y.sin();
    let cx_rot = rot_x.cos();
    let sx_rot = rot_x.sin();
    let cam_dist = 4.8;
    let focal = scale_px * 2.15;

    let transform = |v: Vec3| -> Vec3 {
        let sx = v.x * breath_scale;
        let sy = v.y * breath_scale;
        let sz = v.z * breath_scale;
        // Rotate around Y
        let x1 = sx * cy_rot + sz * sy_rot;
        let z1 = -sx * sy_rot + sz * cy_rot;
        // Rotate around X
        let y2 = sy * cx_rot - z1 * sx_rot;
        let z2 = sy * sx_rot + z1 * cx_rot;
        Vec3 { x: x1, y: y2, z: z2 }
    };

    let light_dir = Vec3 {
        x: 0.32,
        y: 0.72,
        z: 0.61,
    }
    .norm();
    let view_dir = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    struct ProjectedPoly {
        z_avg: f64,
        pts: Vec<[f64; 2]>,
        fill: String,
    }

    let mut projected: Vec<ProjectedPoly> = Vec::with_capacity(quads.len());
    for q in quads {
        let tv = [
            transform(q.verts[0]),
            transform(q.verts[1]),
            transform(q.verts[2]),
            transform(q.verts[3]),
        ];
        let e1 = tv[1].sub(tv[0]);
        let e2 = tv[2].sub(tv[0]);
        let mut n = e1.cross(e2).norm();
        // Ensure outward winding check works cleanly against camera view vector
        let center_v = Vec3 {
            x: (tv[0].x + tv[1].x + tv[2].x + tv[3].x) * 0.25,
            y: (tv[0].y + tv[1].y + tv[2].y + tv[3].y) * 0.25,
            z: (tv[0].z + tv[1].z + tv[2].z + tv[3].z) * 0.25,
        };
        if n.dot(center_v) < 0.0 {
            n = Vec3 {
                x: -n.x,
                y: -n.y,
                z: -n.z,
            };
        }
        if n.z <= -0.08 {
            continue;
        }

        let lambert = n.dot(light_dir).max(0.0);
        let half_v = Vec3 {
            x: light_dir.x + view_dir.x,
            y: light_dir.y + view_dir.y,
            z: light_dir.z + view_dir.z,
        }
        .norm();
        let spec = n.dot(half_v).max(0.0).powf(16.0) * q.metallic;
        let intensity = 0.28 + lambert * 0.68;

        let r = (q.base_rgb[0] * intensity + 255.0 * spec * 0.55).clamp(0.0, 255.0) as u8;
        let g = (q.base_rgb[1] * intensity + 240.0 * spec * 0.55).clamp(0.0, 255.0) as u8;
        let b = (q.base_rgb[2] * intensity + 185.0 * spec * 0.45).clamp(0.0, 255.0) as u8;

        let pts: Vec<[f64; 2]> = tv
            .iter()
            .map(|p| {
                let denom = (cam_dist - p.z).max(0.5);
                let persp = focal / denom;
                [cx + p.x * persp, cy - p.y * persp]
            })
            .collect();

        projected.push(ProjectedPoly {
            z_avg: center_v.z,
            pts,
            fill: format!("rgb({r}, {g}, {b})"),
        });
    }

    // Painter's sort back-to-front (smaller z is farther from camera)
    projected.sort_by(|a, b| a.z_avg.total_cmp(&b.z_avg));

    for poly in projected {
        cmds.push(CanvasDrawCmd::DrawPolygon {
            points: poly.pts,
            fill: poly.fill,
            stroke: Some("rgba(122, 75, 11, 0.22)".to_string()),
            line_width: 0.6,
            alpha: 1.0,
        });
    }
}

fn push_incense_and_offerings(
    cmds: &mut Vec<CanvasDrawCmd>,
    width: f64,
    height: f64,
    time_seconds: f64,
    is_burning: bool,
    incense_progress: f64,
    show_book: bool,
    book_title: Option<&str>,
) {
    let base_x = width / 2.0;
    let base_y = height * 0.72;
    let scale = (width / 400.0).clamp(0.78, 1.2);
    let remaining = (1.0 - incense_progress).clamp(0.16, 1.0);
    let stick_height = 82.0 * remaining * scale;

    // 1. Left offering: Fruit & Flower pedestal
    let left_x = (base_x - 118.0 * scale).max(46.0);
    let offer_y = base_y + 18.0 * scale;
    cmds.push(CanvasDrawCmd::DrawPath {
        ops: vec![
            PathOp::MoveTo {
                x: left_x - 16.0 * scale,
                y: offer_y + 12.0 * scale,
            },
            PathOp::LineTo {
                x: left_x + 16.0 * scale,
                y: offer_y + 12.0 * scale,
            },
            PathOp::LineTo {
                x: left_x + 10.0 * scale,
                y: offer_y - 10.0 * scale,
            },
            PathOp::LineTo {
                x: left_x - 10.0 * scale,
                y: offer_y - 10.0 * scale,
            },
            PathOp::Close,
        ],
        fill: Some("#D4AF37".to_string()),
        fill_linear: None,
        fill_radial: None,
        stroke: Some("#8B5A2B".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
        line_cap_round: false,
    });
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx: left_x,
        cy: offer_y - 14.0 * scale,
        rx: 34.0 * scale,
        ry: 8.0 * scale,
        rotation: 0.0,
        fill: Some("#FFD76B".to_string()),
        stroke: Some("#8B5A2B".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
    });
    for &(dx, dy, r, color) in &[
        (-13.0, -21.0, 9.5, "#E53935"),
        (13.0, -21.0, 9.5, "#F4511E"),
        (0.0, -29.0, 11.0, "#FFB300"),
    ] {
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: left_x + dx * scale,
            cy: offer_y + dy * scale,
            r: r * scale,
            fill: Some(color.to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 1.0,
        });
    }
    for &sign in &[-1.0_f64, 1.0] {
        let lx = left_x + sign * 24.0 * scale;
        let ly = offer_y - 18.0 * scale;
        for &pa in &[-0.5_f64, 0.0, 0.5] {
            cmds.push(CanvasDrawCmd::DrawEllipse {
                cx: lx + pa.sin() * 6.0 * scale,
                cy: ly - 5.0 * scale,
                rx: 4.5 * scale,
                ry: 9.0 * scale,
                rotation: pa,
                fill: Some("#F8BBD0".to_string()),
                stroke: Some("#E91E63".to_string()),
                line_width: 0.6,
                blur: 0.0,
                alpha: 1.0,
            });
        }
    }

    // 2. Right offering: Butter Lamp with sinusoidal flicker
    let right_x = (base_x + 118.0 * scale).min(width - 46.0);
    cmds.push(CanvasDrawCmd::DrawPath {
        ops: vec![
            PathOp::MoveTo {
                x: right_x - 16.0 * scale,
                y: offer_y + 12.0 * scale,
            },
            PathOp::LineTo {
                x: right_x + 16.0 * scale,
                y: offer_y + 12.0 * scale,
            },
            PathOp::LineTo {
                x: right_x + 9.0 * scale,
                y: offer_y - 10.0 * scale,
            },
            PathOp::LineTo {
                x: right_x - 9.0 * scale,
                y: offer_y - 10.0 * scale,
            },
            PathOp::Close,
        ],
        fill: Some("#D4AF37".to_string()),
        fill_linear: None,
        fill_radial: None,
        stroke: Some("#8B5A2B".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
        line_cap_round: false,
    });
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx: right_x,
        cy: offer_y - 18.0 * scale,
        rx: 24.0 * scale,
        ry: 7.5 * scale,
        rotation: 0.0,
        fill: Some("#FFD76B".to_string()),
        stroke: Some("#8B5A2B".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
    });
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx: right_x,
        cy: offer_y - 18.0 * scale,
        rx: 20.0 * scale,
        ry: 5.5 * scale,
        rotation: 0.0,
        fill: Some("#FDE89F".to_string()),
        stroke: None,
        line_width: 0.0,
        blur: 0.0,
        alpha: 1.0,
    });
    if is_burning {
        let noise_x = (time_seconds * 5.0).sin() * 0.5
            + (time_seconds * 3.1).sin() * 0.3
            + (time_seconds * 7.3).sin() * 0.2;
        let noise_y = (time_seconds * 4.0).sin() * 0.5
            + (time_seconds * 2.7).sin() * 0.3
            + (time_seconds * 8.1).sin() * 0.2;
        let fx = right_x + noise_x * 2.5 * scale;
        let fy_base = offer_y - 21.0 * scale;
        let fy_tip = fy_base - (22.0 + noise_y * 3.0) * scale;
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: right_x,
            cy: fy_base - 10.0 * scale,
            r: 22.0 * scale,
            fill: Some("rgba(255, 153, 0, 0.35)".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 10.0,
            alpha: 1.0,
        });
        cmds.push(CanvasDrawCmd::DrawPath {
            ops: vec![
                PathOp::MoveTo { x: fx, y: fy_tip },
                PathOp::QuadraticTo {
                    cx: right_x + 10.0 * scale,
                    cy: fy_base - 8.0 * scale,
                    x: right_x + 5.0 * scale,
                    y: fy_base + 2.0 * scale,
                },
                PathOp::QuadraticTo {
                    cx: right_x,
                    cy: fy_base + 5.0 * scale,
                    x: right_x - 5.0 * scale,
                    y: fy_base + 2.0 * scale,
                },
                PathOp::QuadraticTo {
                    cx: right_x - 10.0 * scale,
                    cy: fy_base - 8.0 * scale,
                    x: fx,
                    y: fy_tip,
                },
                PathOp::Close,
            ],
            fill: Some("#FF9900".to_string()),
            fill_linear: None,
            fill_radial: None,
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: false,
        });
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: right_x + noise_x * 1.0 * scale,
            cy: fy_base - 5.0 * scale,
            r: 3.5 * scale,
            fill: Some("#FFFFFF".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 0.0,
            alpha: 0.95,
        });
    }

    // 3. Center bronze incense burner
    let burner_w = 90.0 * scale;
    let burner_h = 28.0 * scale;
    cmds.push(CanvasDrawCmd::DrawPath {
        ops: vec![
            PathOp::MoveTo {
                x: base_x - burner_w * 0.45,
                y: base_y + 4.0 * scale,
            },
            PathOp::QuadraticTo {
                cx: base_x - burner_w * 0.36,
                cy: base_y + burner_h,
                x: base_x,
                y: base_y + burner_h * 1.12,
            },
            PathOp::QuadraticTo {
                cx: base_x + burner_w * 0.36,
                cy: base_y + burner_h,
                x: base_x + burner_w * 0.45,
                y: base_y + 4.0 * scale,
            },
            PathOp::Close,
        ],
        fill: None,
        fill_linear: Some(LinearGradient {
            x0: base_x - burner_w * 0.5,
            y0: base_y,
            x1: base_x + burner_w * 0.5,
            y1: base_y + burner_h,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "#4A2111".to_string(),
                },
                ColorStop {
                    offset: 0.5,
                    color: "#9A5A24".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "#2A1208".to_string(),
                },
            ],
        }),
        fill_radial: None,
        stroke: Some("rgba(212, 175, 55, 0.65)".to_string()),
        line_width: 1.2,
        blur: 0.0,
        alpha: 1.0,
        line_cap_round: false,
    });
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx: base_x,
        cy: base_y + 4.0 * scale,
        rx: burner_w * 0.5,
        ry: 7.5 * scale,
        rotation: 0.0,
        fill: Some("#D4AF37".to_string()),
        stroke: Some("#8B5A2B".to_string()),
        line_width: 1.0,
        blur: 0.0,
        alpha: 1.0,
    });
    cmds.push(CanvasDrawCmd::DrawEllipse {
        cx: base_x,
        cy: base_y + 4.5 * scale,
        rx: burner_w * 0.36,
        ry: 4.0 * scale,
        rotation: 0.0,
        fill: Some("#6A5A45".to_string()),
        stroke: None,
        line_width: 0.0,
        blur: 0.0,
        alpha: 1.0,
    });

    // 4. Three burning incense sticks + 11 sinusoidal smoke wisps each
    for &dx in &[-14.0_f64, 0.0, 14.0] {
        let sx = base_x + dx * scale;
        let sy_base = base_y + 2.0 * scale;
        let sy_tip = sy_base - stick_height;

        cmds.push(CanvasDrawCmd::DrawLine {
            x0: sx,
            y0: sy_base,
            x1: sx,
            y1: sy_tip,
            stroke: None,
            gradient: Some(LinearGradient {
                x0: sx,
                y0: sy_base,
                x1: sx,
                y1: sy_tip,
                stops: vec![
                    ColorStop {
                        offset: 0.0,
                        color: "#5A2E16".to_string(),
                    },
                    ColorStop {
                        offset: 0.5,
                        color: "#E7B45F".to_string(),
                    },
                    ColorStop {
                        offset: 1.0,
                        color: "#2B1509".to_string(),
                    },
                ],
            }),
            line_width: 3.6 * scale,
            blur: 0.0,
            alpha: 1.0,
            line_cap_round: true,
        });

        if is_burning {
            cmds.push(CanvasDrawCmd::DrawCircle {
                cx: sx,
                cy: sy_tip,
                r: 5.5 * scale,
                fill: Some("rgba(255, 107, 26, 0.6)".to_string()),
                stroke: None,
                line_width: 0.0,
                blur: 4.0,
                alpha: 1.0,
            });
            cmds.push(CanvasDrawCmd::DrawCircle {
                cx: sx,
                cy: sy_tip,
                r: 2.2 * scale,
                fill: Some("#FFE6A3".to_string()),
                stroke: None,
                line_width: 0.0,
                blur: 0.0,
                alpha: 1.0,
            });

            // 11 sinusoidal smoke wisps matching `_drawFixedSmoke` in `zen_room_2d_elements.dart`
            for i in 0..11 {
                let t = (time_seconds * 0.18 + (i as f64) / 11.0 + dx * 0.006).rem_euclid(1.0);
                let wx = sx + (t * PI * 2.0 + dx).sin() * (7.0 + t * 26.0);
                let wy = sy_tip - t * 122.0 - (i as f64) * 3.2;
                let wr = 2.4 + t * 9.5;
                let opacity = ((1.0 - t) * 0.17 + 0.025).clamp(0.0, 0.20);
                cmds.push(CanvasDrawCmd::DrawCircle {
                    cx: wx,
                    cy: wy,
                    r: wr,
                    fill: Some(format!("rgba(235, 229, 214, {opacity:.3})")),
                    stroke: None,
                    line_width: 0.0,
                    blur: 3.0 + t * 7.0,
                    alpha: 1.0,
                });
            }
        }
    }

    // 5. Thread-bound Sutra Book resting above burner
    if show_book {
        let title = book_title.unwrap_or("选择功课");
        let bw = 64.0 * scale;
        let bh = 86.0 * scale;
        let bx = base_x - bw / 2.0;
        let by = base_y - stick_height - bh - 12.0 * scale;

        cmds.push(CanvasDrawCmd::DrawLinearRect {
            x: bx,
            y: by,
            w: bw,
            h: bh,
            gradient: Some(LinearGradient {
                x0: bx,
                y0: by,
                x1: bx + bw,
                y1: by + bh,
                stops: vec![
                    ColorStop {
                        offset: 0.0,
                        color: "#1C2C54".to_string(),
                    },
                    ColorStop {
                        offset: 1.0,
                        color: "#080E1E".to_string(),
                    },
                ],
            }),
            fill: None,
            corner_radius: 4.0,
            alpha: 0.95,
        });
        cmds.push(CanvasDrawCmd::DrawLinearRect {
            x: bx + 10.0 * scale,
            y: by + 10.0 * scale,
            w: 22.0 * scale,
            h: bh - 20.0 * scale,
            gradient: None,
            fill: Some("#FDFBF7".to_string()),
            corner_radius: 2.0,
            alpha: 1.0,
        });
        cmds.push(CanvasDrawCmd::DrawText {
            text: title.to_string(),
            x: bx + 21.0 * scale,
            y: by + bh * 0.52,
            color: "#2C1E1A".to_string(),
            font_size: 9.5 * scale,
            bold: true,
            align: "center".to_string(),
            shadow_color: None,
            shadow_blur: 0.0,
            alpha: 1.0,
        });
    }
}

/// Renders the 2D/3D Zen Meditation Room into declarative `CanvasDrawCmd`s.
pub fn render_zen_scene(
    state: &ZenSceneState,
    width: f64,
    height: f64,
    is_3d: bool,
    show_book: bool,
    book_title: Option<&str>,
) -> Vec<CanvasDrawCmd> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let mut cmds = Vec::with_capacity(280);
    let t = state.time_seconds;
    let cx = width / 2.0;
    let halo_cy = height * 0.285;
    let base_radius = (width.min(height) * 0.30).clamp(108.0, 230.0);
    let pulse = 0.5 + (t * 1.05).sin() * 0.5;

    // 1. Zen backdrop
    cmds.push(CanvasDrawCmd::DrawLinearRect {
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
        gradient: Some(LinearGradient {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: height,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "#0B0E14".to_string(),
                },
                ColorStop {
                    offset: 0.55,
                    color: "#1B263B".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "#0B0E14".to_string(),
                },
            ],
        }),
        fill: None,
        corner_radius: 0.0,
        alpha: 1.0,
    });
    cmds.push(CanvasDrawCmd::DrawBackgroundImage {
        asset: "zen_room_backdrop_2d.webp".to_string(),
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
        alpha: if is_3d { 0.45 } else { 0.92 },
    });

    // 2. Radial golden halo + 4 concentric halo rings
    cmds.push(CanvasDrawCmd::DrawRadialCircle {
        cx,
        cy: halo_cy,
        r: base_radius * (1.25 + pulse * 0.03),
        gradient: RadialGradient {
            fx: cx,
            fy: halo_cy,
            r0: 0.0,
            cx,
            cy: halo_cy,
            r1: base_radius * 1.25,
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: "rgba(255, 248, 212, 0.47)".to_string(),
                },
                ColorStop {
                    offset: 0.55,
                    color: "rgba(255, 215, 107, 0.27)".to_string(),
                },
                ColorStop {
                    offset: 1.0,
                    color: "rgba(255, 215, 107, 0.0)".to_string(),
                },
            ],
        },
        blur: 10.0,
        alpha: 1.0,
    });

    for i in 0..4 {
        let r = base_radius * (0.68 + i as f64 * 0.18 + pulse * 0.01);
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx,
            cy: halo_cy,
            r,
            fill: None,
            stroke: Some("rgba(255, 242, 168, 0.66)".to_string()),
            line_width: 0.8 + i as f64 * 0.25,
            blur: 1.5,
            alpha: 1.0,
        });
    }

    // 3. 8 orbiting 5-petal lotuses (0.55 rad/s circumambulating vs 0.12 rad/s calm)
    let speed = if state.auto_rotate {
        ZEN_ORBIT_CIRCUMAMBULATE_RAD_PER_SEC
    } else {
        ZEN_ORBIT_CALM_RAD_PER_SEC
    };
    let orbit_rot = t * speed;
    for i in 0..8 {
        let angle = orbit_rot + (i as f64) * PI / 4.0;
        let lx = cx + angle.cos() * base_radius;
        let ly = halo_cy + angle.sin() * base_radius;
        let lscale = 0.72 + (i % 2) as f64 * 0.18;

        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: lx,
            cy: ly,
            r: 16.0 * lscale,
            fill: Some("rgba(255, 215, 107, 0.20)".to_string()),
            stroke: None,
            line_width: 0.0,
            blur: 7.0,
            alpha: 1.0,
        });

        for &pa in &[-0.55_f64, -0.25, 0.0, 0.25, 0.55] {
            let total_a = angle + pa;
            let c = total_a.cos();
            let s = total_a.sin();
            let rot = |px: f64, py: f64| -> (f64, f64) {
                (lx + px * c - py * s, ly + px * s + py * c)
            };
            let (p0x, p0y) = rot(0.0, 7.0 * lscale);
            let (c1x, c1y) = rot(-6.0 * lscale, 0.0);
            let (tx, ty) = rot(0.0, -13.0 * lscale);
            let (c2x, c2y) = rot(6.0 * lscale, 0.0);
            let (g0x, g0y) = rot(0.0, -13.0 * lscale);
            let (g1x, g1y) = rot(0.0, 7.0 * lscale);

            cmds.push(CanvasDrawCmd::DrawPath {
                ops: vec![
                    PathOp::MoveTo { x: p0x, y: p0y },
                    PathOp::QuadraticTo {
                        cx: c1x,
                        cy: c1y,
                        x: tx,
                        y: ty,
                    },
                    PathOp::QuadraticTo {
                        cx: c2x,
                        cy: c2y,
                        x: p0x,
                        y: p0y,
                    },
                    PathOp::Close,
                ],
                fill: None,
                fill_linear: Some(LinearGradient {
                    x0: g0x,
                    y0: g0y,
                    x1: g1x,
                    y1: g1y,
                    stops: vec![
                        ColorStop {
                            offset: 0.0,
                            color: "#FFFFFF".to_string(),
                        },
                        ColorStop {
                            offset: 1.0,
                            color: "#FFCC45".to_string(),
                        },
                    ],
                }),
                fill_radial: None,
                stroke: None,
                line_width: 0.0,
                blur: 0.0,
                alpha: 1.0,
                line_cap_round: false,
            });
        }
    }

    // 4. Breathing & floating Buddha figure (2D vector or 3D golden lotus-throne mesh)
    let breath = 1.0 + zen_breathing_offset(t);
    let float_y = zen_float_offset(t);
    let buddha_cy = height * 0.36 + float_y;
    let buddha_scale = ((height * 0.54).clamp(240.0, 520.0) / 340.0) * breath;

    if is_3d {
        project_3d_buddha_mesh(
            &mut cmds,
            cx,
            buddha_cy,
            (width.min(height) * 0.25).clamp(90.0, 180.0),
            state.buddha_rot_y,
            state.buddha_rot_x,
            breath,
        );
    } else {
        push_2d_buddha(&mut cmds, cx, buddha_cy, buddha_scale);
    }

    // 5. 20 golden sun motes
    for i in 0..20 {
        let phase = t * 0.05 + (i as f64) * 0.41;
        let mx = ((phase * 1.7).sin() * 0.5 + 0.5) * width;
        let my = ((phase * 1.3).cos() * 0.5 + 0.5) * height * 0.72;
        let opacity = 0.08 + ((phase * 3.0).sin() * 0.5 + 0.5) * 0.16;
        cmds.push(CanvasDrawCmd::DrawCircle {
            cx: mx,
            cy: my,
            r: 1.2 + (i % 3) as f64 * 0.9,
            fill: Some(format!("rgba(255, 241, 182, {opacity:.3})")),
            stroke: None,
            line_width: 0.0,
            blur: 2.0,
            alpha: 1.0,
        });
    }

    // 6. Offerings: Fruit/Flower, Butter Lamp, Bronze Incense Burner, 3 Sticks + 11 Smoke Wisps, Sutra Book
    push_incense_and_offerings(
        &mut cmds,
        width,
        height,
        t,
        state.is_burning,
        state.incense_progress,
        show_book,
        book_title,
    );

    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_lat_lng_3d_front_and_back() {
        // Facing Beijing (lat ~40, lng ~116) with rotation at 116 -> visible front hemisphere
        let pt = project_lat_lng_3d(39.9, 116.4, 200.0, 200.0, 100.0, 116.4, 0.0, -0.16)
            .expect("front point must project");
        assert!((pt.x - 200.0).abs() < 1.0);
        assert!(pt.y < 200.0);
        assert!(pt.z > 0.7);

        // Opposite side of Earth should be culled
        let back = project_lat_lng_3d(0.0, -64.0, 200.0, 200.0, 100.0, 116.0, 0.0, -0.16);
        assert!(back.is_none());
    }

    #[test]
    fn globe_rotation_and_focus_interpolation_follow_exact_rates() {
        let mut globe = GlobeSceneState::default();
        // Idle: 4.5 deg/sec
        globe.advance(1000.0, 1000.0);
        assert!((globe.rotation_lon_deg - 112.5).abs() < 0.01);

        // Sending: 1.15 deg/sec + shortest-longitude focus over 2000ms
        globe.add_beam(39.9, 116.4, 35.6, 139.7, Some("日本".into()), 1000.0, 1800.0);
        globe.advance(1000.0, 2000.0); // t=0.5 -> cubic ease out = 0.875
        let expected_delta = shortest_longitude_delta(112.5, 139.7) * 0.875;
        assert!((globe.rotation_lon_deg - normalize_longitude(112.5 + expected_delta)).abs() < 0.05);

        // After 2000ms focus completes, resumes sending rate 1.15 deg/sec
        globe.advance(1000.0, 3000.0);
        assert!((globe.rotation_lon_deg - 139.7).abs() < 0.1);
        globe.advance(1000.0, 4000.0);
        assert!((globe.rotation_lon_deg - (139.7 + SENDING_ROTATION_DEG_PER_SEC)).abs() < 0.05);
    }

    #[test]
    fn zen_scene_2d_and_3d_emit_expected_commands() {
        let mut zen = ZenSceneState::default();
        zen.is_burning = true;
        zen.auto_rotate = true;
        zen.advance(500.0);

        let cmds_2d = render_zen_scene(&zen, 420.0, 760.0, false, true, Some("金刚经"));
        assert!(cmds_2d.len() > 80);

        let cmds_3d = render_zen_scene(&zen, 420.0, 760.0, true, true, Some("心经"));
        let polygon_count = cmds_3d
            .iter()
            .filter(|c| matches!(c, CanvasDrawCmd::DrawPolygon { .. }))
            .count();
        assert!(polygon_count > 40, "3D Buddha mesh should emit polygons, got {polygon_count}");
    }
}
