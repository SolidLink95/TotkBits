//! Mip generation through GDI+ (`gdiplus.dll`), the resampler behind
//! System.Drawing: HighQualityBicubic interpolation, HighQuality pixel
//! offsets and compositing, SourceCopy, TileFlipXY wrapping. Each level is
//! produced from the previous one, so a chain built here is byte-identical
//! to one built by a .NET tool with those settings.
//!
//! Off Windows the module degrades to the `image` crate's triangle filter.
use image::RgbaImage;
use std::io;

#[cfg(windows)]
mod native {
    use super::*;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Graphics::GdiPlus::*;

    static STARTUP: OnceLock<Result<(), String>> = OnceLock::new();
    /// `PixelFormat32bppARGB`: 32 bpp, alpha, canonical, index 10.
    const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

    fn startup() -> io::Result<()> {
        STARTUP
            .get_or_init(|| unsafe {
                let input = GdiplusStartupInput {
                    GdiplusVersion: 1,
                    DebugEventCallback: 0,
                    SuppressBackgroundThread: BOOL(0),
                    SuppressExternalCodecs: BOOL(0),
                };
                let mut token = 0usize;
                let status = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
                if status == Ok {
                    Result::Ok(())
                } else {
                    Err(format!("GdiplusStartup failed with status {}", status.0))
                }
            })
            .clone()
            .map_err(io::Error::other)
    }

    fn check(status: Status, what: &str) -> io::Result<()> {
        if status == Ok {
            io::Result::Ok(())
        } else {
            Err(io::Error::other(format!(
                "{what} failed with GDI+ status {}",
                status.0
            )))
        }
    }

    struct Bitmap(*mut GpBitmap);
    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe {
                GdipDisposeImage(self.0.cast());
            }
        }
    }

    /// A fresh 32bppARGB bitmap (`new Bitmap(width, height)`).
    fn create(width: u32, height: u32) -> io::Result<Bitmap> {
        let mut bitmap = std::ptr::null_mut();
        unsafe {
            check(
                GdipCreateBitmapFromScan0(
                    width as i32,
                    height as i32,
                    0,
                    PIXEL_FORMAT_32BPP_ARGB,
                    None,
                    &mut bitmap,
                ),
                "GdipCreateBitmapFromScan0",
            )?;
        }
        io::Result::Ok(Bitmap(bitmap))
    }

    /// Copies raw 4-byte texels in or out through LockBits (`ImageToByte` /
    /// `GetBitmap`); the byte order is passed through untouched.
    fn lock_copy(
        bitmap: &Bitmap,
        width: u32,
        height: u32,
        buffer: &mut [u8],
        write: bool,
    ) -> io::Result<()> {
        let rect = Rect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        let mut data = BitmapData::default();
        let flags = if write {
            ImageLockModeWrite
        } else {
            ImageLockModeRead
        };
        unsafe {
            check(
                GdipBitmapLockBits(
                    bitmap.0,
                    &rect,
                    flags.0 as u32,
                    PIXEL_FORMAT_32BPP_ARGB,
                    &mut data,
                ),
                "GdipBitmapLockBits",
            )?;
            let stride = data.Stride as usize;
            let row_bytes = width as usize * 4;
            for y in 0..height as usize {
                let row = data.Scan0.cast::<u8>().add(y * stride);
                let target = &mut buffer[y * row_bytes..(y + 1) * row_bytes];
                if write {
                    std::ptr::copy_nonoverlapping(target.as_ptr(), row, row_bytes);
                } else {
                    std::ptr::copy_nonoverlapping(row, target.as_mut_ptr(), row_bytes);
                }
            }
            check(
                GdipBitmapUnlockBits(bitmap.0, &mut data),
                "GdipBitmapUnlockBits",
            )?;
        }
        io::Result::Ok(())
    }

    pub fn resize(source: &RgbaImage, width: u32, height: u32) -> io::Result<RgbaImage> {
        startup()?;
        let (sw, sh) = (source.width(), source.height());
        let src = create(sw, sh)?;
        let mut pixels = source.as_raw().clone();
        lock_copy(&src, sw, sh, &mut pixels, true)?;

        let dst = create(width, height)?;
        unsafe {
            let (mut hres, mut vres) = (0f32, 0f32);
            check(
                GdipGetImageHorizontalResolution(src.0.cast(), &mut hres),
                "GdipGetImageHorizontalResolution",
            )?;
            check(
                GdipGetImageVerticalResolution(src.0.cast(), &mut vres),
                "GdipGetImageVerticalResolution",
            )?;
            check(
                GdipBitmapSetResolution(dst.0, hres, vres),
                "GdipBitmapSetResolution",
            )?;

            let mut graphics = std::ptr::null_mut();
            check(
                GdipGetImageGraphicsContext(dst.0.cast(), &mut graphics),
                "GdipGetImageGraphicsContext",
            )?;
            struct Graphics(*mut GpGraphics);
            impl Drop for Graphics {
                fn drop(&mut self) {
                    unsafe {
                        GdipDeleteGraphics(self.0);
                    }
                }
            }
            let graphics = Graphics(graphics);
            check(
                GdipSetCompositingMode(graphics.0, CompositingModeSourceCopy),
                "GdipSetCompositingMode",
            )?;
            check(
                GdipSetCompositingQuality(graphics.0, CompositingQualityHighQuality),
                "GdipSetCompositingQuality",
            )?;
            check(
                GdipSetInterpolationMode(graphics.0, InterpolationModeHighQualityBicubic),
                "GdipSetInterpolationMode",
            )?;
            check(
                GdipSetSmoothingMode(graphics.0, SmoothingModeHighQuality),
                "GdipSetSmoothingMode",
            )?;
            check(
                GdipSetPixelOffsetMode(graphics.0, PixelOffsetModeHighQuality),
                "GdipSetPixelOffsetMode",
            )?;

            let mut attributes = std::ptr::null_mut();
            check(
                GdipCreateImageAttributes(&mut attributes),
                "GdipCreateImageAttributes",
            )?;
            struct Attributes(*mut GpImageAttributes);
            impl Drop for Attributes {
                fn drop(&mut self) {
                    unsafe {
                        GdipDisposeImageAttributes(self.0);
                    }
                }
            }
            let attributes = Attributes(attributes);
            // ImageAttributes.SetWrapMode(mode) = (mode, Color.Black, clamp: false)
            check(
                GdipSetImageAttributesWrapMode(
                    attributes.0,
                    WrapModeTileFlipXY,
                    0xFF00_0000,
                    false,
                ),
                "GdipSetImageAttributesWrapMode",
            )?;
            check(
                GdipDrawImageRectRectI(
                    graphics.0,
                    src.0.cast(),
                    0,
                    0,
                    width as i32,
                    height as i32,
                    0,
                    0,
                    sw as i32,
                    sh as i32,
                    UnitPixel,
                    attributes.0,
                    0,
                    std::ptr::null_mut(),
                ),
                "GdipDrawImageRectRectI",
            )?;
        }
        let mut out = vec![0u8; width as usize * height as usize * 4];
        lock_copy(&dst, width, height, &mut out, false)?;
        RgbaImage::from_raw(width, height, out)
            .ok_or_else(|| io::Error::other("GDI+ returned a short buffer"))
    }
}

/// Resamples `source` to `width` x `height`.
pub fn resize(source: &RgbaImage, width: u32, height: u32) -> io::Result<RgbaImage> {
    let width = width.max(1);
    let height = height.max(1);
    #[cfg(windows)]
    {
        native::resize(source, width, height)
    }
    #[cfg(not(windows))]
    {
        Ok(image::imageops::resize(
            source,
            width,
            height,
            image::imageops::FilterType::Triangle,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_a_flat_image_without_changing_its_colour() {
        let mut source = RgbaImage::new(8, 8);
        for p in source.pixels_mut() {
            p.0 = [40, 120, 200, 255];
        }
        let half = resize(&source, 4, 4).unwrap();
        assert_eq!(half.dimensions(), (4, 4));
        assert!(half.pixels().all(|p| p.0 == [40, 120, 200, 255]));
    }
}
