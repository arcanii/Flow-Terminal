//! libwebp's animation encoder (`WebPAnimEncoder`), through libwebp-sys,
//! taking each frame as it comes: libwebp keeps only what it has encoded and
//! the frame before, so a long recording holds no raw frames. The `webp`
//! crate's `AnimEncoder`, which make_movie uses, keeps every frame until the
//! end instead.

use std::ffi::CStr;
use std::ptr;

use libwebp_sys as sys;

/// An animated WebP being written, a frame at a time, looping forever.
pub struct AnimEncoder {
    enc: *mut sys::WebPAnimEncoder,
    config: sys::WebPConfig,
    /// The same at libwebp's fastest method, for a frame added in a hurry.
    fast: sys::WebPConfig,
    width: u32,
    height: u32,
    /// The last frame's time, which every later one must pass.
    last_ms: Option<i32>,
}

impl AnimEncoder {
    /// A canvas `width` × `height`, encoded as `how` says.
    pub fn new(width: u32, height: u32, how: &super::Encoding) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 16383 || height > 16383 {
            return Err(format!("a canvas {width} × {height}, which WebP can't hold"));
        }
        // SAFETY: each struct is filled in by libwebp's own initializer before
        // it's read, the version passed is the headers' libwebp-sys was made
        // from, and the encoder made is freed in Drop.
        unsafe {
            let mut options = std::mem::MaybeUninit::<sys::WebPAnimEncoderOptions>::zeroed();
            if sys::WebPAnimEncoderOptionsInitInternal(options.as_mut_ptr(), sys::WEBP_MUX_ABI_VERSION as i32) == 0 {
                return Err("libwebp's animation options wouldn't start".into());
            }
            let mut options = options.assume_init();
            options.anim_params.loop_count = 0;
            options.anim_params.bgcolor = 0;
            // A whole frame every `keyframes`: in between, a frame is
            // only the rectangles that changed, and a lossy encoder takes
            // a pixel that changed a little as unchanged, so a liquid
            // that moves slowly everywhere drifts into a patchwork until
            // a whole frame puts it right. Each frame from kmin on is
            // encoded twice, as a whole frame and as its changes, for
            // libwebp to keep the smaller, so kmin is as close to kmax as
            // it goes: half way, as at first, doubled half the frames'
            // work.
            options.kmax = how.keyframes;
            options.kmin = how.keyframes - 1;
            let mut config = std::mem::MaybeUninit::<sys::WebPConfig>::zeroed();
            if sys::WebPConfigInitInternal(config.as_mut_ptr(), sys::WebPPreset::WEBP_PRESET_DEFAULT, how.quality, sys::WEBP_ENCODER_ABI_VERSION as i32) == 0 {
                return Err("libwebp's configuration wouldn't start".into());
            }
            let mut config = config.assume_init();
            config.method = how.method.clamp(0, 6);
            config.lossless = how.lossless as i32;
            // A second thread for what libwebp can do beside the encoding.
            config.thread_level = 1;
            let fast = sys::WebPConfig { method: 0, ..config };
            if sys::WebPValidateConfig(&config) == 0 || sys::WebPValidateConfig(&fast) == 0 {
                return Err("libwebp turned its configuration down".into());
            }
            let enc = sys::WebPAnimEncoderNewInternal(width as i32, height as i32, &options, sys::WEBP_MUX_ABI_VERSION as i32);
            if enc.is_null() {
                return Err("libwebp's animation encoder wouldn't start".into());
            }
            Ok(AnimEncoder { enc, config, fast, width, height, last_ms: None })
        }
    }

    /// Adds a frame of straight RGBA, `width` × `height` as the canvas, shown
    /// from `at_ms` (after the last frame's) until the next; in a `hurry`,
    /// at libwebp's fastest method, which takes each frame as libwebp's
    /// settings may, frame by frame.
    pub fn add(&mut self, rgba: &[u8], at_ms: i32, hurry: bool) -> Result<(), String> {
        if rgba.len() != self.width as usize * self.height as usize * 4 {
            return Err(format!("a frame of {} bytes for a canvas {} × {}", rgba.len(), self.width, self.height));
        }
        if self.last_ms.is_some_and(|last| at_ms <= last) {
            return Err(format!("a frame at {at_ms} ms, not after the last"));
        }
        // SAFETY: the picture is initialized by libwebp, takes a copy of the
        // pixels (whose length is checked above) and is freed after the
        // encoder has taken what it needs from it.
        unsafe {
            let mut picture = std::mem::MaybeUninit::<sys::WebPPicture>::zeroed();
            if sys::WebPPictureInitInternal(picture.as_mut_ptr(), sys::WEBP_ENCODER_ABI_VERSION as i32) == 0 {
                return Err("libwebp's picture wouldn't start".into());
            }
            let mut picture = picture.assume_init();
            picture.use_argb = 1;
            picture.width = self.width as i32;
            picture.height = self.height as i32;
            if sys::WebPPictureImportRGBA(&mut picture, rgba.as_ptr(), self.width as i32 * 4) == 0 {
                sys::WebPPictureFree(&mut picture);
                return Err("libwebp couldn't take the frame's pixels".into());
            }
            let config = if hurry { &self.fast } else { &self.config };
            let added = sys::WebPAnimEncoderAdd(self.enc, &mut picture, at_ms, config);
            sys::WebPPictureFree(&mut picture);
            if added == 0 {
                return Err(self.error("adding a frame"));
            }
        }
        self.last_ms = Some(at_ms);
        Ok(())
    }

    /// Ends the animation at `end_ms` (the last frame shown until then) and
    /// hands back the file's bytes.
    pub fn finish(mut self, end_ms: i32) -> Result<Vec<u8>, String> {
        if self.last_ms.is_none() {
            return Err("a recording with no frames".into());
        }
        let end_ms = end_ms.max(self.last_ms.unwrap_or(0) + 1);
        // SAFETY: a null frame ends the animation, as libwebp documents;
        // the data it assembles is copied out and freed with WebPFree.
        unsafe {
            if sys::WebPAnimEncoderAdd(self.enc, ptr::null_mut(), end_ms, ptr::null()) == 0 {
                return Err(self.error("ending the animation"));
            }
            let mut data = sys::WebPData::default();
            if sys::WebPAnimEncoderAssemble(self.enc, &mut data) == 0 {
                return Err(self.error("assembling the file"));
            }
            let bytes = std::slice::from_raw_parts(data.bytes, data.size).to_vec();
            sys::WebPDataClear(&mut data);
            self.last_ms = None;
            Ok(bytes)
        }
    }

    /// What libwebp says went wrong, after `doing`.
    fn error(&mut self, doing: &str) -> String {
        // SAFETY: the encoder is live; libwebp hands back a string of its own,
        // or null.
        let said = unsafe {
            let s = sys::WebPAnimEncoderGetError(self.enc);
            if s.is_null() { String::new() } else { CStr::from_ptr(s).to_string_lossy().into_owned() }
        };
        if said.is_empty() { format!("libwebp failed {doing}") } else { format!("libwebp failed {doing}: {said}") }
    }
}

impl Drop for AnimEncoder {
    fn drop(&mut self) {
        // SAFETY: made by WebPAnimEncoderNewInternal, freed once.
        unsafe { sys::WebPAnimEncoderDelete(self.enc) }
    }
}
