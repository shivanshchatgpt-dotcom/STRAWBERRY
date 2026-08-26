//! Screen Memory — perceptual hash utilities for near-duplicate detection.
//! Uses a simple but effective DCT-based pHash (8x8 -> 64-bit).

use image::{DynamicImage, GenericImageView, Pixel};

/// Compute 64-bit perceptual hash (pHash) of an image.
/// Returns 16-char hex string (64 bits).
pub fn perceptual_hash(img: &DynamicImage) -> String {
    // 1. Resize to 32x32 grayscale
    let gray = img.grayscale().resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
    
    // 2. Convert to 32x32 f32 matrix
    let mut pixels = [[0.0f32; 32]; 32];
    for y in 0..32 {
        for x in 0..32 {
            let pixel = gray.get_pixel(x, y);
            // Handle different pixel formats - grayscale image should have Luma
            let v = pixel.to_luma().0[0] as f32;
            pixels[y as usize][x as usize] = v;
        }
    }
    
    // 3. 2D DCT (simple separable implementation)
    let mut dct = [[0.0f32; 32]; 32];
    // Row-wise DCT
    for y in 0..32 {
        for u in 0..32 {
            let mut sum = 0.0;
            for x in 0..32 {
                sum += pixels[y][x] * ((std::f32::consts::PI * u as f32 * (2.0 * x as f32 + 1.0) / 64.0).cos());
            }
            dct[y][u] = sum * if u == 0 { 1.0 / (32.0_f32.sqrt()) } else { (2.0 / 32.0_f32).sqrt() };
        }
    }
    // Column-wise DCT
    let mut dct2 = [[0.0f32; 32]; 32];
    for x in 0..32 {
        for v in 0..32 {
            let mut sum = 0.0;
            for y in 0..32 {
                sum += dct[y][x] * ((std::f32::consts::PI * v as f32 * (2.0 * y as f32 + 1.0) / 64.0).cos());
            }
            dct2[v][x] = sum * if v == 0 { 1.0 / (32.0_f32.sqrt()) } else { (2.0 / 32.0_f32).sqrt() };
        }
    }
    
    // 4. Take top-left 8x8 (low frequencies), compute median
    let mut coeffs = Vec::with_capacity(64);
    for y in 0..8 {
        for x in 0..8 {
            coeffs.push(dct2[y][x]);
        }
    }
    let median = {
        let mut sorted = coeffs.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[32]
    };
    
    // 5. Build 64-bit hash: 1 if coeff > median else 0
    let mut hash: u64 = 0;
    for (i, &c) in coeffs.iter().enumerate() {
        if c > median {
            hash |= 1u64 << i;
        }
    }
    
    format!("{:016x}", hash)
}

pub fn hamming_distance(a: &str, b: &str) -> Option<u8> {
    let a_val = u64::from_str_radix(a, 16).ok();
    let b_val = u64::from_str_radix(b, 16).ok();
    match (a_val, b_val) {
        (Some(a), Some(b)) => Some((a ^ b).count_ones() as u8),
        _ => None,
    }
}

/// Check if two images are visually similar (hamming distance <= threshold).
/// Default threshold: 10 (quite strict).
#[allow(dead_code)]
pub fn is_similar(a: &str, b: &str, threshold: u8) -> bool {
    hamming_distance(a, b).map(|d| d <= threshold).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    
    #[test]
    fn identical_images_zero_distance() {
        let img = ImageBuffer::from_fn(100, 100, |_, _| Rgb([128, 128, 128]));
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        let h1 = perceptual_hash(&dyn_img);
        let h2 = perceptual_hash(&dyn_img);
        assert_eq!(h1, h2);
        assert_eq!(hamming_distance(&h1, &h2), Some(0));
    }
    
    #[test]
    fn different_images_nonzero_distance() {
        let img1 = image::ImageBuffer::from_fn(100, 100, |_, _| image::Rgb([0, 0, 0]));
        let img2 = image::ImageBuffer::from_fn(100, 100, |_, _| image::Rgb([255, 255, 255]));
        let h1 = perceptual_hash(&image::DynamicImage::ImageRgb8(img1));
        let h2 = perceptual_hash(&image::DynamicImage::ImageRgb8(img2));
        let dist = hamming_distance(&h1, &h2).unwrap();
        assert!(dist > 0);
    }
}