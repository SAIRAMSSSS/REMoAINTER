#![allow(dead_code)]
use egui::{pos2, vec2, Color32, Painter, Rect, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    // Toolbar Tools
    Brush,
    Eraser,
    Projection,
    PolygonFill,
    Smudge,
    CloneStamp,
    Eyedropper,
    Decal,

    // Dock Strip Panels
    Settings,
    TextureSetList,
    Layers,
    Display,
    Shader,
    History,
    Log,
}

pub fn draw_icon(painter: &Painter, rect: Rect, icon: IconType, color: Color32) {
    let center = rect.center();
    let s = (rect.width().min(rect.height()) * 0.38).clamp(6.0, 14.0);
    let stroke = Stroke::new(1.3_f32, color);
    let stroke_thick = Stroke::new(1.8_f32, color);

    match icon {
        IconType::Brush => {
            // Paint Brush: Diagonal handle and bristle tip
            let p_tip = pos2(center.x - s * 0.65, center.y + s * 0.65);
            let p_ferrule1 = pos2(center.x - s * 0.20, center.y + s * 0.20);
            let p_ferrule2 = pos2(center.x + s * 0.05, center.y - s * 0.05);
            let p_end = pos2(center.x + s * 0.70, center.y - s * 0.70);

            // Wooden handle
            painter.line_segment([p_ferrule2, p_end], stroke_thick);
            // Metal ferrule
            painter.line_segment([p_ferrule1, p_ferrule2], Stroke::new(2.4_f32, color));
            // Bristles (triangle tip)
            painter.line_segment([p_tip, pos2(p_ferrule1.x - s * 0.15, p_ferrule1.y)], stroke);
            painter.line_segment([p_tip, pos2(p_ferrule1.x, p_ferrule1.y + s * 0.15)], stroke);
        }

        IconType::Eraser => {
            // Beveled angled eraser block
            let r = Rect::from_center_size(center, vec2(s * 1.3, s * 0.9));
            painter.rect_stroke(r, 2.0_f32, stroke);
            // Dividing line between eraser halves
            painter.line_segment(
                [pos2(center.x, r.top()), pos2(center.x, r.bottom())],
                stroke,
            );
            // Angled ground smudge
            painter.line_segment(
                [
                    pos2(r.left() - 2.0, r.bottom() + 2.0),
                    pos2(r.right() + 2.0, r.bottom() + 2.0),
                ],
                Stroke::new(1.0_f32, color),
            );
        }

        IconType::Projection => {
            // Camera frustum projecting onto stencil frame
            let frame =
                Rect::from_center_size(pos2(center.x + s * 0.25, center.y), vec2(s * 0.9, s * 1.2));
            painter.rect_stroke(frame, 1.0, stroke);
            // Projector / lens on left
            let cam = pos2(center.x - s * 0.65, center.y);
            painter.circle_filled(cam, 2.5, color);
            // Projection rays
            painter.line_segment([cam, pos2(frame.left(), frame.top())], stroke);
            painter.line_segment([cam, pos2(frame.left(), frame.bottom())], stroke);
        }

        IconType::PolygonFill => {
            // Wireframe triangle with shaded interior
            let p0 = pos2(center.x, center.y - s * 0.75);
            let p1 = pos2(center.x - s * 0.75, center.y + s * 0.65);
            let p2 = pos2(center.x + s * 0.75, center.y + s * 0.65);
            painter.line_segment([p0, p1], stroke);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p0], stroke);
            // Central fill accent
            painter.circle_filled(pos2(center.x, center.y + s * 0.15), 3.0, color);
        }

        IconType::Smudge => {
            // Flowing wave streak
            let y1 = center.y - s * 0.35;
            let y2 = center.y + s * 0.35;
            painter.line_segment(
                [
                    pos2(center.x - s * 0.7, y1),
                    pos2(center.x - s * 0.2, y1 - 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x - s * 0.2, y1 - 2.0),
                    pos2(center.x + s * 0.2, y1 + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x + s * 0.2, y1 + 2.0),
                    pos2(center.x + s * 0.7, y1),
                ],
                stroke,
            );

            painter.line_segment(
                [pos2(center.x - s * 0.5, y2), pos2(center.x, y2 - 2.0)],
                stroke,
            );
            painter.line_segment(
                [pos2(center.x, y2 - 2.0), pos2(center.x + s * 0.5, y2)],
                stroke,
            );
        }

        IconType::CloneStamp => {
            // Classic rubber stamp: handle knob and base plate
            let base = Rect::from_center_size(
                pos2(center.x, center.y + s * 0.45),
                vec2(s * 1.3, s * 0.35),
            );
            painter.rect_filled(base, 1.0, color);
            // Handle stem
            painter.line_segment(
                [
                    pos2(center.x, base.top()),
                    pos2(center.x, center.y - s * 0.35),
                ],
                stroke_thick,
            );
            // Top knob
            painter.circle_filled(pos2(center.x, center.y - s * 0.45), 3.5, color);
        }

        IconType::Eyedropper => {
            // 45-degree pipette
            let p_bulb = pos2(center.x + s * 0.55, center.y - s * 0.55);
            let p_tube_top = pos2(center.x + s * 0.20, center.y - s * 0.20);
            let p_tip = pos2(center.x - s * 0.60, center.y + s * 0.60);

            painter.circle_filled(p_bulb, 3.2, color);
            painter.line_segment([p_bulb, p_tube_top], stroke_thick);
            painter.line_segment([p_tube_top, p_tip], stroke);
            // Droplet
            painter.circle_filled(pos2(p_tip.x - 2.0, p_tip.y + 2.0), 1.5, color);
        }

        IconType::Decal => {
            // Square decal frame with a stamped glyph pattern inside
            let frame = Rect::from_min_max(
                pos2(center.x - s * 0.75, center.y - s * 0.75),
                pos2(center.x + s * 0.75, center.y + s * 0.75),
            );
            painter.rect_stroke(frame, 2.0_f32, stroke);
            let patch = Rect::from_min_max(
                pos2(center.x - s * 0.35, center.y - s * 0.35),
                pos2(center.x + s * 0.35, center.y + s * 0.35),
            );
            painter.rect_filled(patch, 4.0, color);
            painter.line_segment(
                [
                    pos2(center.x, center.y - s * 0.8),
                    pos2(center.x, center.y - s * 0.55),
                ],
                stroke_thick,
            );
        }

        IconType::Settings => {
            // 8-tooth gear
            let r_outer = s * 0.75;
            let r_inner = s * 0.45;
            painter.circle_stroke(center, r_inner, stroke);
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::PI / 4.0;
                let c_a = angle.cos();
                let s_a = angle.sin();
                let p1 = pos2(center.x + c_a * r_inner, center.y + s_a * r_inner);
                let p2 = pos2(center.x + c_a * r_outer, center.y + s_a * r_outer);
                painter.line_segment([p1, p2], stroke);
            }
        }

        IconType::TextureSetList => {
            // Two overlapping stacked UV tile rectangles
            let r_back = Rect::from_center_size(
                pos2(center.x + s * 0.20, center.y - s * 0.20),
                vec2(s * 0.85, s * 0.85),
            );
            let r_front = Rect::from_center_size(
                pos2(center.x - s * 0.15, center.y + s * 0.15),
                vec2(s * 0.85, s * 0.85),
            );
            painter.rect_stroke(
                r_back,
                1.0_f32,
                Stroke::new(1.0_f32, Color32::from_gray(120)),
            );
            painter.rect_filled(r_front, 1.0_f32, Color32::from_rgb(33, 33, 33));
            painter.rect_stroke(r_front, 1.0_f32, stroke);
        }

        IconType::Layers => {
            // Three stacked horizontal layer bars
            let w = s * 1.35;
            let h = 2.4;
            let y0 = center.y - s * 0.55;
            let y1 = center.y;
            let y2 = center.y + s * 0.55;

            painter.rect_filled(
                Rect::from_center_size(pos2(center.x, y0), vec2(w, h)),
                1.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(pos2(center.x, y1), vec2(w, h)),
                1.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(pos2(center.x, y2), vec2(w, h)),
                1.0,
                color,
            );
        }

        IconType::Display => {
            // Desktop monitor display with stand
            let screen =
                Rect::from_center_size(pos2(center.x, center.y - s * 0.18), vec2(s * 1.3, s * 0.9));
            painter.rect_stroke(screen, 1.5, stroke);
            // Stand stem
            painter.line_segment(
                [
                    pos2(center.x, screen.bottom()),
                    pos2(center.x, center.y + s * 0.55),
                ],
                stroke,
            );
            // Stand foot
            painter.line_segment(
                [
                    pos2(center.x - s * 0.45, center.y + s * 0.55),
                    pos2(center.x + s * 0.45, center.y + s * 0.55),
                ],
                stroke,
            );
        }

        IconType::Shader => {
            // Globe / Sphere with equator and central meridian
            let r = s * 0.75;
            painter.circle_stroke(center, r, stroke);
            // Equator line
            painter.line_segment(
                [pos2(center.x - r, center.y), pos2(center.x + r, center.y)],
                stroke,
            );
            // Meridian line
            painter.line_segment(
                [pos2(center.x, center.y - r), pos2(center.x, center.y + r)],
                stroke,
            );
        }

        IconType::History => {
            // Circular stopwatch / clock
            let r = s * 0.72;
            painter.circle_stroke(center, r, stroke);
            // Clock hands at 10:10
            painter.line_segment(
                [center, pos2(center.x - r * 0.45, center.y - r * 0.45)],
                stroke,
            );
            painter.line_segment(
                [center, pos2(center.x + r * 0.55, center.y - r * 0.15)],
                stroke,
            );
            // Top crown knob
            painter.line_segment(
                [
                    pos2(center.x, center.y - r),
                    pos2(center.x, center.y - r - 2.0),
                ],
                stroke,
            );
        }

        IconType::Log => {
            // Terminal console with >_ prompt
            let r = Rect::from_center_size(center, vec2(s * 1.3, s * 0.95));
            painter.rect_stroke(r, 1.0, stroke);
            // Header bar
            painter.line_segment(
                [
                    pos2(r.left(), r.top() + 4.0),
                    pos2(r.right(), r.top() + 4.0),
                ],
                stroke,
            );
            // `>` prompt
            let p_mid = pos2(r.left() + 4.0, center.y + 1.0);
            painter.line_segment([pos2(r.left() + 2.0, center.y - 1.5), p_mid], stroke);
            painter.line_segment([p_mid, pos2(r.left() + 2.0, center.y + 3.5)], stroke);
            // `_` cursor
            painter.line_segment(
                [
                    pos2(r.left() + 7.0, center.y + 3.5),
                    pos2(r.left() + 11.0, center.y + 3.5),
                ],
                stroke,
            );
        }
    }
}
