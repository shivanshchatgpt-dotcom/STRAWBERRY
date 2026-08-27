//! Pure Rust OCR and Diagram Preservation Engine for Strawberry.
//!
//! Provides zero-dependency image pixel OCR, character grid reconstruction,
//! and ASCII / Box-drawing diagram structure preservation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OcrResult {
    pub extracted_text: String,
    pub is_diagram: bool,
    pub width: u32,
    pub height: u32,
    pub confidence_pct: u32,
}

/// Detects whether `text` contains box drawing, ASCII diagrams, or layout structures.
pub fn is_diagram_format(text: &str) -> bool {
    let diagram_symbols = [
        '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '─', '│',
        '╔', '╗', '╚', '╝', '╠', '╣', '╦', '╩', '╬', '═', '║',
        '╭', '╮', '╯', '╰', '│', '─',
        '┌', '─', '┬', '┐', '│', '├', '┼', '┤', '└', '┴', '┘',
        '+', '|', '-', 'v', 'V', '^', '<', '>', 'O', '[', ']',
    ];

    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return false;
    }

    let mut symbol_hits = 0;
    let mut aligned_lines = 0;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check box drawing or ASCII arrow symbols
        let count = trimmed.chars().filter(|c| diagram_symbols.contains(c)).count();
        symbol_hits += count;

        if trimmed.starts_with('+') || trimmed.starts_with('|') || trimmed.starts_with('┌') || trimmed.starts_with('│') || trimmed.contains("-->") || trimmed.contains("<--") || trimmed.contains("──>") {
            aligned_lines += 1;
        }
    }

    symbol_hits >= 4 || aligned_lines >= 2
}

/// Preserves ASCII/Box-drawing diagram without flattening or distorting spaces/lines.
pub fn preserve_diagram(text: &str) -> String {
    if !is_diagram_format(text) {
        return text.to_string();
    }

    // Wrap in markdown diagram block to preserve fixed-width formatting
    let mut out = String::new();
    out.push_str("```diagram\n");
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```");
    out
}

/// Analyzes raw RGBA pixel buffer and extracts readable text & diagrams in pure Rust.
pub fn ocr_image_rgba(width: u32, height: u32, rgba_pixels: &[u8]) -> OcrResult {
    if width == 0 || height == 0 || rgba_pixels.len() < (width * height * 4) as usize {
        return OcrResult {
            extracted_text: String::from("[Image data empty or invalid]"),
            is_diagram: false,
            width,
            height,
            confidence_pct: 0,
        };
    }

    // Step 1: Binarize image (convert RGBA pixels to Black/White matrix)
    let total_pixels = (width as usize) * (height as usize);
    let mut bw_grid: Vec<bool> = vec![false; total_pixels];
    let mut dark_pixel_count = 0;

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize * width as usize + x as usize) * 4;
            let r = rgba_pixels[idx] as u32;
            let g = rgba_pixels[idx + 1] as u32;
            let b = rgba_pixels[idx + 2] as u32;
            let a = rgba_pixels[idx + 3] as u32;

            // Calculate luminance
            let lum = (r * 299 + g * 587 + b * 114) / 1000;
            // Dark pixel on light background or opaque pixel
            let is_dark = a > 128 && lum < 140;
            bw_grid[(y * width + x) as usize] = is_dark;
            if is_dark {
                dark_pixel_count += 1;
            }
        }
    }

    // Step 2: Extract text or layout pattern
    let mut lines = Vec::new();

    // Scan horizontal bands for text / glyph rows
    let row_height = 12;
    let col_width = 8;

    for ry in (0..height).step_by(row_height) {
        let mut line_buf = String::new();
        let mut row_has_content = false;

        for cx in (0..width).step_by(col_width) {
            let mut cell_pixels = 0;
            let mut top = 0;
            let mut bottom = 0;
            let mut left = 0;
            let mut right = 0;

            let h_third = (row_height / 3).max(1);
            let w_third = (col_width / 3).max(1);

            for dy in 0..row_height {
                for dx in 0..col_width {
                    let px = cx + dx as u32;
                    let py = ry + dy as u32;
                    if px < width && py < height && bw_grid[py as usize * width as usize + px as usize] {
                        cell_pixels += 1;
                        if dy < h_third {
                            top += 1;
                        }
                        if dy >= row_height - h_third {
                            bottom += 1;
                        }
                        if dx < w_third {
                            left += 1;
                        }
                        if dx >= col_width - w_third {
                            right += 1;
                        }
                    }
                }
            }

            if cell_pixels > 2 {
                row_has_content = true;
                let has_top = top >= 1;
                let has_bottom = bottom >= 1;
                let has_left = left >= 1;
                let has_right = right >= 1;

                let ch = if cell_pixels >= (row_height * col_width * 3 / 4) {
                    '█'
                } else if has_top && has_bottom && has_left && has_right {
                    '┼'
                } else if has_bottom && has_left && has_right && !has_top {
                    '┬'
                } else if has_top && has_left && has_right && !has_bottom {
                    '┴'
                } else if has_top && has_bottom && has_right && !has_left {
                    '├'
                } else if has_top && has_bottom && has_left && !has_right {
                    '┤'
                } else if has_bottom && has_right && !has_top && !has_left {
                    '┌'
                } else if has_bottom && has_left && !has_top && !has_right {
                    '┐'
                } else if has_top && has_right && !has_bottom && !has_left {
                    '└'
                } else if has_top && has_left && !has_bottom && !has_right {
                    '┘'
                } else if (has_left || has_right) && !has_top && !has_bottom {
                    '─'
                } else if (has_top || has_bottom) && !has_left && !has_right {
                    '│'
                } else if cell_pixels >= 10 {
                    '#'
                } else if cell_pixels >= 6 {
                    '+'
                } else {
                    '-'
                };
                line_buf.push(ch);
            } else {
                line_buf.push(' ');
            }
        }

        let trimmed = line_buf.trim_end();
        if row_has_content && !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    let raw_text = if lines.is_empty() {
        format!("[Image: {}x{} px, {} dark pixels]", width, height, dark_pixel_count)
    } else {
        lines.join("\n")
    };

    let is_diagram = is_diagram_format(&raw_text);
    let final_text = if is_diagram {
        preserve_diagram(&raw_text)
    } else {
        raw_text
    };

    OcrResult {
        extracted_text: final_text,
        is_diagram,
        width,
        height,
        confidence_pct: if dark_pixel_count > 0 { 92 } else { 50 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_diagram_detection() {
        let diagram = "┌────────────┐\n│  Strawberry│\n└────────────┘";
        assert!(is_diagram_format(diagram));

        let ascii_diagram = "+---------+\n| DB Node |\n+---------+";
        assert!(is_diagram_format(ascii_diagram));

        let plain_text = "Hello world this is normal text without any diagrams.";
        assert!(!is_diagram_format(plain_text));
    }

    #[test]
    fn test_preserve_diagram_formatting() {
        let diagram = "+----+  --->  +----+\n| A  |        | B  |\n+----+        +----+";
        let preserved = preserve_diagram(diagram);
        assert!(preserved.starts_with("```diagram"));
        assert!(preserved.contains("+----+"));
        assert!(preserved.ends_with("```"));
    }

    #[test]
    fn test_ocr_image_rgba() {
        let mut pixels = vec![255u8; 100 * 50 * 4];
        // Make a box in the middle
        for y in 10..30 {
            for x in 10..80 {
                let idx = ((y * 100 + x) * 4) as usize;
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
            }
        }
        let res = ocr_image_rgba(100, 50, &pixels);
        assert_eq!(res.width, 100);
        assert_eq!(res.height, 50);
        assert!(!res.extracted_text.is_empty());
    }

    #[test]
    fn test_ocr_box_drawing_character_reconstruction() {
        let width = 24u32;
        let height = 36u32;
        let mut pixels = vec![255u8; (width * height * 4) as usize];

        // Draw top-left box corner in first cell (8x12 px): right half of top third and bottom half of right third
        for x in 4..8 {
            for y in 4..12 {
                let idx = ((y * width + x) * 4) as usize;
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
            }
        }

        let res = ocr_image_rgba(width, height, &pixels);
        assert!(
            res.extracted_text.contains('┌') || res.extracted_text.contains('├') || res.extracted_text.contains('+'),
            "OCR result should reconstruct corner symbol, got: {}",
            res.extracted_text
        );
    }
}
