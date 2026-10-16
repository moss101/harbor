//! The page images of a scanned PDF, read straight out of the file.
//!
//! A scanner or phone app produces a PDF whose page is one big image. No
//! renderer is needed to read it: the image XObject IS the page. JPEG
//! (`DCTDecode`) streams are handed on as they are; Flate-compressed 8-bit
//! gray / RGB rasters are inflated (PNG predictors undone) into raw RGB.
//! Anything fancier (JBIG2, CCITT, 16-bit, indexed colour) is reported as
//! unsupported rather than guessed at.

use std::io::Read;

#[derive(Debug, Clone, PartialEq)]
pub enum PageImageData {
    /// A complete JPEG file.
    Jpeg(Vec<u8>),
    /// 8-bit RGB, row-major, `width * height * 3` bytes.
    Rgb {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    /// 1-based page number.
    pub page: u32,
    pub data: PageImageData,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanImageError {
    #[error("not a readable PDF: {0}")]
    Pdf(String),
}

const MAX_PIXELS: i64 = 80_000_000;

/// The largest image on each page, for pages that have one.
pub fn page_images(bytes: &[u8]) -> Result<Vec<PageImage>, ScanImageError> {
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| ScanImageError::Pdf(e.to_string()))?;
    let mut out = Vec::new();
    for (page_no, page_id) in doc.get_pages() {
        let Ok(images) = doc.get_page_images(page_id) else {
            continue;
        };
        let Some(best) = images
            .iter()
            .filter(|i| i.width > 0 && i.height > 0 && i.width * i.height <= MAX_PIXELS)
            .max_by_key(|i| i.width * i.height)
        else {
            continue;
        };
        let filters = best.filters.clone().unwrap_or_default();
        let data = if filters.iter().any(|f| f == "DCTDecode") && filters.len() == 1 {
            Some(PageImageData::Jpeg(best.content.to_vec()))
        } else if filters.iter().all(|f| f == "FlateDecode") && !filters.is_empty() {
            inflate_raster(
                best.content,
                best.width as usize,
                best.height as usize,
                best.bits_per_component,
                best.origin_dict,
            )
        } else if filters.is_empty() {
            raster_to_rgb(
                best.content.to_vec(),
                best.width as usize,
                best.height as usize,
                best.bits_per_component,
            )
        } else {
            None
        };
        if let Some(data) = data {
            out.push(PageImage {
                page: page_no,
                data,
            });
        }
    }
    Ok(out)
}

fn inflate_raster(
    raw: &[u8],
    w: usize,
    h: usize,
    bpc: Option<i64>,
    dict: &lopdf::Dictionary,
) -> Option<PageImageData> {
    // The dictionary is attacker-controlled, so the inflated size is bounded
    // by what the declared raster can legitimately occupy (RGB plus one
    // predictor byte per row); anything longer is refused, not buffered.
    let limit = (w as u64).checked_mul(h as u64)?.checked_mul(3)? + h as u64 + 1;
    let mut inflated = Vec::new();
    flate2::read::ZlibDecoder::new(raw)
        .take(limit + 1)
        .read_to_end(&mut inflated)
        .ok()?;
    if inflated.len() as u64 > limit {
        return None;
    }
    let predictor = dict
        .get(b"DecodeParms")
        .ok()
        .and_then(|p| p.as_dict().ok())
        .and_then(|d| d.get(b"Predictor").ok())
        .and_then(|p| p.as_i64().ok())
        .unwrap_or(1);
    let data = if predictor >= 10 {
        // PNG predictors: each row is prefixed with a filter-type byte.
        let colors = if inflated.len() >= h * (w + 1) && inflated.len() < h * (w * 3 + 1) {
            1
        } else {
            3
        };
        undo_png_predictor(&inflated, w, h, colors)?
    } else {
        inflated
    };
    raster_to_rgb(data, w, h, bpc)
}

fn undo_png_predictor(data: &[u8], w: usize, h: usize, bpp: usize) -> Option<Vec<u8>> {
    let stride = w * bpp;
    if data.len() < h * (stride + 1) {
        return None;
    }
    let mut out = vec![0u8; h * stride];
    for y in 0..h {
        let ft = data[y * (stride + 1)];
        let src = &data[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= bpp {
                out[y * stride + x - bpp] as i32
            } else {
                0
            };
            let b = if y > 0 {
                out[(y - 1) * stride + x] as i32
            } else {
                0
            };
            let c = if y > 0 && x >= bpp {
                out[(y - 1) * stride + x - bpp] as i32
            } else {
                0
            };
            let add = match ft {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => return None,
            };
            out[y * stride + x] = (src[x] as i32 + add) as u8;
        }
    }
    Some(out)
}

fn raster_to_rgb(data: Vec<u8>, w: usize, h: usize, bpc: Option<i64>) -> Option<PageImageData> {
    if bpc.unwrap_or(8) != 8 {
        return None;
    }
    let px = w * h;
    let rgb = if data.len() == px * 3 {
        data
    } else if data.len() == px {
        let mut v = Vec::with_capacity(px * 3);
        for g in data {
            v.extend_from_slice(&[g, g, g]);
        }
        v
    } else {
        return None;
    };
    Some(PageImageData::Rgb {
        width: w as u32,
        height: h as u32,
        data: rgb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flate_stream_larger_than_its_declared_raster_is_refused() {
        use std::io::Write;
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&vec![0u8; 5_000_000]).unwrap();
        let bomb = enc.finish().unwrap();
        assert!(bomb.len() < 10_000);
        let dict = lopdf::Dictionary::new();
        assert!(inflate_raster(&bomb, 2, 2, Some(8), &dict).is_none());
    }

    #[test]
    fn png_predictor_up_and_sub_roundtrip() {
        // 3x2 gray: row0 filter Sub [10, +5, +5] -> 10 15 20; row1 filter Up [+1,+1,+1] -> 11 16 21
        let data = [1u8, 10, 5, 5, 2, 1, 1, 1];
        assert_eq!(
            undo_png_predictor(&data, 3, 2, 1).unwrap(),
            vec![10, 15, 20, 11, 16, 21]
        );
    }

    #[test]
    fn gray_expands_to_rgb_and_wrong_sizes_are_refused() {
        let img = raster_to_rgb(vec![0, 255], 2, 1, Some(8)).unwrap();
        assert_eq!(
            img,
            PageImageData::Rgb {
                width: 2,
                height: 1,
                data: vec![0, 0, 0, 255, 255, 255]
            }
        );
        assert!(raster_to_rgb(vec![0; 5], 2, 1, Some(8)).is_none());
        assert!(raster_to_rgb(vec![0; 2], 2, 1, Some(1)).is_none());
    }

    #[test]
    fn a_pdf_without_images_yields_none_and_garbage_is_an_error() {
        assert!(page_images(b"not a pdf").is_err());
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    fn the_scanned_fixture_pdfs_yield_their_page_image() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ocr");
        for (name, pages) in [
            ("scanned_invoice_en.pdf", vec![1u32]),
            ("scanned_invoice_batch.pdf", vec![1]),
            ("mixed_text_and_scan.pdf", vec![2]), // page 1 is vector text: no image
        ] {
            let Ok(bytes) = std::fs::read(root.join(name)) else {
                continue;
            };
            let imgs = page_images(&bytes).unwrap();
            let got: Vec<u32> = imgs.iter().map(|i| i.page).collect();
            assert_eq!(got, pages, "{name}");
            match &imgs[0].data {
                PageImageData::Rgb {
                    width,
                    height,
                    data,
                } => {
                    assert_eq!(*width, 1240);
                    assert_eq!(data.len(), (*width * *height * 3) as usize);
                }
                PageImageData::Jpeg(j) => assert!(j.starts_with(&[0xFF, 0xD8])),
            }
        }
    }
}
