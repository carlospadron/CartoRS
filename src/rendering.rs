use image::{ImageBuffer, Rgb, RgbImage};
use crate::datasource::geometry::{Feature, Geometry};
use crate::datasource::Bbox;

/// Default fill color (semi-transparent blue rendered as solid on RGB)
const FILL_COLOR: Rgb<u8> = Rgb([180, 200, 230]);
/// Default stroke color (dark blue)
const STROKE_COLOR: Rgb<u8> = Rgb([30, 60, 120]);
/// Default point color (red)
const POINT_COLOR: Rgb<u8> = Rgb([200, 50, 50]);
/// Background color (white)
const BG_COLOR: Rgb<u8> = Rgb([255, 255, 255]);

/// Render features to a PNG image.
pub fn render_map(
    features: &[Feature],
    bbox: &Bbox,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let mut img: RgbImage = ImageBuffer::from_pixel(width, height, BG_COLOR);

    for feature in features {
        render_geometry(&mut img, &feature.geometry, bbox, width, height);
    }

    let mut buffer = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buffer, image::ImageOutputFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    Ok(buffer.into_inner())
}

/// Convert world coordinates to pixel coordinates.
fn world_to_pixel(x: f64, y: f64, bbox: &Bbox, width: u32, height: u32) -> (i32, i32) {
    let px = ((x - bbox[0]) / (bbox[2] - bbox[0]) * width as f64) as i32;
    // Y axis is inverted in image coordinates
    let py = ((bbox[3] - y) / (bbox[3] - bbox[1]) * height as f64) as i32;
    (px, py)
}

fn render_geometry(
    img: &mut RgbImage,
    geom: &Geometry,
    bbox: &Bbox,
    width: u32,
    height: u32,
) {
    match geom {
        Geometry::Point(x, y) => {
            let (px, py) = world_to_pixel(*x, *y, bbox, width, height);
            draw_circle(img, px, py, 4, POINT_COLOR);
        }
        Geometry::LineString(coords) => {
            let pixels: Vec<(i32, i32)> = coords
                .iter()
                .map(|(x, y)| world_to_pixel(*x, *y, bbox, width, height))
                .collect();
            for window in pixels.windows(2) {
                draw_line(img, window[0].0, window[0].1, window[1].0, window[1].1, STROKE_COLOR);
            }
        }
        Geometry::Polygon(rings) => {
            // Fill the polygon (outer ring only for simplicity)
            if let Some(outer_ring) = rings.first() {
                let pixels: Vec<(i32, i32)> = outer_ring
                    .iter()
                    .map(|(x, y)| world_to_pixel(*x, *y, bbox, width, height))
                    .collect();
                fill_polygon(img, &pixels, FILL_COLOR);
                // Draw outline
                for window in pixels.windows(2) {
                    draw_line(img, window[0].0, window[0].1, window[1].0, window[1].1, STROKE_COLOR);
                }
                // Close the ring
                if pixels.len() >= 2 {
                    let first = pixels[0];
                    let last = pixels[pixels.len() - 1];
                    draw_line(img, last.0, last.1, first.0, first.1, STROKE_COLOR);
                }
            }
        }
        Geometry::MultiPoint(points) => {
            for (x, y) in points {
                let (px, py) = world_to_pixel(*x, *y, bbox, width, height);
                draw_circle(img, px, py, 4, POINT_COLOR);
            }
        }
        Geometry::MultiLineString(lines) => {
            for line in lines {
                let pixels: Vec<(i32, i32)> = line
                    .iter()
                    .map(|(x, y)| world_to_pixel(*x, *y, bbox, width, height))
                    .collect();
                for window in pixels.windows(2) {
                    draw_line(img, window[0].0, window[0].1, window[1].0, window[1].1, STROKE_COLOR);
                }
            }
        }
        Geometry::MultiPolygon(polys) => {
            for rings in polys {
                render_geometry(img, &Geometry::Polygon(rings.clone()), bbox, width, height);
            }
        }
    }
}

/// Draw a filled circle at (cx, cy) with given radius.
fn draw_circle(img: &mut RgbImage, cx: i32, cy: i32, radius: i32, color: Rgb<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && px < w && py >= 0 && py < h {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

/// Draw a line using Bresenham's algorithm.
fn draw_line(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    loop {
        if x >= 0 && x < w && y >= 0 && y < h {
            img.put_pixel(x as u32, y as u32, color);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if x == x1 {
                break;
            }
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            if y == y1 {
                break;
            }
            err += dx;
            y += sy;
        }
    }
}

/// Simple scanline polygon fill.
fn fill_polygon(img: &mut RgbImage, vertices: &[(i32, i32)], color: Rgb<u8>) {
    if vertices.len() < 3 {
        return;
    }

    let (w, h) = (img.width() as i32, img.height() as i32);

    // Find bounding box of polygon
    let min_y = vertices.iter().map(|v| v.1).min().unwrap().max(0);
    let max_y = vertices.iter().map(|v| v.1).max().unwrap().min(h - 1);

    for y in min_y..=max_y {
        let mut intersections = Vec::new();
        let n = vertices.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let (y0, y1) = (vertices[i].1, vertices[j].1);
            let (x0, x1) = (vertices[i].0, vertices[j].0);

            if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                let x = x0 + (y - y0) as i32 * (x1 - x0) / (y1 - y0);
                intersections.push(x);
            }
        }

        intersections.sort();

        for chunk in intersections.chunks(2) {
            if chunk.len() == 2 {
                let x_start = chunk[0].max(0);
                let x_end = chunk[1].min(w - 1);
                for x in x_start..=x_end {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_world_to_pixel() {
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let (px, py) = world_to_pixel(50.0, 50.0, &bbox, 200, 200);
        assert_eq!(px, 100);
        assert_eq!(py, 100);

        let (px, py) = world_to_pixel(0.0, 0.0, &bbox, 200, 200);
        assert_eq!(px, 0);
        assert_eq!(py, 200);

        let (px, py) = world_to_pixel(100.0, 100.0, &bbox, 200, 200);
        assert_eq!(px, 200);
        assert_eq!(py, 0);
    }

    #[test]
    fn test_render_empty_map() {
        let features: Vec<Feature> = vec![];
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let result = render_map(&features, &bbox, 256, 256);
        assert!(result.is_ok());
        let png_data = result.unwrap();
        // Check it's a valid PNG (starts with PNG magic bytes)
        assert!(png_data.len() > 8);
        assert_eq!(&png_data[1..4], b"PNG");
    }

    #[test]
    fn test_render_points() {
        let features = vec![
            Feature {
                geometry: Geometry::Point(50.0, 50.0),
                properties: HashMap::new(),
            },
            Feature {
                geometry: Geometry::Point(25.0, 75.0),
                properties: HashMap::new(),
            },
        ];
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let result = render_map(&features, &bbox, 256, 256);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_polygon() {
        let features = vec![Feature {
            geometry: Geometry::Polygon(vec![vec![
                (10.0, 10.0),
                (90.0, 10.0),
                (90.0, 90.0),
                (10.0, 90.0),
                (10.0, 10.0),
            ]]),
            properties: HashMap::new(),
        }];
        let bbox = [0.0, 0.0, 100.0, 100.0];
        let result = render_map(&features, &bbox, 256, 256);
        assert!(result.is_ok());
    }
}
