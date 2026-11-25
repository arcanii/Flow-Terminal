//! A recording's frames, taken from what the renderer draws: the terminal
//! area of the frame about to be shown, copied into one of a few staging
//! buffers and read back once the GPU is done with it, a frame or two later,
//! never waited for. With every buffer still on its way, a frame isn't
//! taken, so a slow GPU loses frames from the recording rather than holding
//! up the window. Where the surface can't be copied from, the frame is drawn
//! again into a texture of the window's size for it (`Renderer::render`).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::record::Frame;

/// Buffers in turn: one being written, one on its way back, one spare.
const SLOTS: usize = 3;

const FREE: u8 = 0;
const WAITING: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;

/// A rectangle of the window, in its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Crop {
    /// Kept inside a window `width` × `height`; none where nothing's left.
    pub fn within(self, width: u32, height: u32) -> Option<Crop> {
        let x = self.x.min(width);
        let y = self.y.min(height);
        let w = self.w.min(width - x);
        let h = self.h.min(height - y);
        (w > 0 && h > 0).then_some(Crop { x, y, w, h })
    }
}

struct Slot {
    buffer: Option<wgpu::Buffer>,
    /// The buffer's crop and padded row, as it was made for.
    size: (u32, u32),
    row: u32,
    state: Arc<AtomicU8>,
    at: Duration,
    bgra: bool,
    /// Copied into this frame, to be mapped once it's submitted.
    copied: bool,
}

/// The staging buffers a recording reads frames back through.
pub struct Capture {
    slots: Vec<Slot>,
}

impl Capture {
    pub fn new() -> Capture {
        let slots = (0..SLOTS)
            .map(|_| Slot { buffer: None, size: (0, 0), row: 0, state: Arc::new(AtomicU8::new(FREE)), at: Duration::ZERO, bgra: false, copied: false })
            .collect();
        Capture { slots }
    }

    /// Whether a buffer is free for the next frame.
    pub fn has_room(&self) -> bool {
        self.slots.iter().any(|s| s.state.load(Ordering::Acquire) == FREE && !s.copied)
    }

    /// Records the copy of `crop` of `texture` into a free buffer, made or
    /// remade for its size, in `encoder`; false where none is free.
    pub fn copy(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture, crop: Crop, bgra: bool, at: Duration) -> bool {
        let Some(slot) = self.slots.iter_mut().find(|s| s.state.load(Ordering::Acquire) == FREE && !s.copied) else { return false };
        let row = (crop.w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        if slot.buffer.is_none() || slot.size != (crop.w, crop.h) {
            slot.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("recording readback"),
                size: (row * crop.h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
            slot.size = (crop.w, crop.h);
            slot.row = row;
        }
        let buffer = slot.buffer.as_ref().expect("made above");
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x: crop.x, y: crop.y, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(crop.h) } },
            wgpu::Extent3d { width: crop.w, height: crop.h, depth_or_array_layers: 1 },
        );
        slot.at = at;
        slot.bgra = bgra;
        slot.copied = true;
        true
    }

    /// Asks for the buffers copied into this frame back, once it's been
    /// submitted (a buffer can't be mapped while a submission still to come
    /// writes it).
    pub fn map_copied(&mut self) {
        for slot in self.slots.iter_mut().filter(|s| s.copied) {
            slot.copied = false;
            let Some(buffer) = slot.buffer.as_ref() else { continue };
            slot.state.store(WAITING, Ordering::Release);
            let state = slot.state.clone();
            buffer.map_async(wgpu::MapMode::Read, .., move |done| {
                state.store(if done.is_ok() { READY } else { FAILED }, Ordering::Release);
            });
        }
    }

    /// The frames whose buffers have come back, oldest first, without
    /// waiting for any still on their way.
    pub fn take(&mut self, device: &wgpu::Device) -> Vec<Frame> {
        if self.slots.iter().all(|s| s.state.load(Ordering::Acquire) == FREE) {
            return Vec::new();
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut frames = Vec::new();
        for slot in &mut self.slots {
            match slot.state.load(Ordering::Acquire) {
                READY => {
                    let Some(buffer) = slot.buffer.as_ref() else { continue };
                    let (w, h) = slot.size;
                    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
                    if let Ok(data) = buffer.get_mapped_range(..) {
                        for y in 0..h as usize {
                            let from = y * slot.row as usize;
                            pixels.extend_from_slice(&data[from..from + w as usize * 4]);
                        }
                    }
                    buffer.unmap();
                    slot.state.store(FREE, Ordering::Release);
                    if pixels.len() == (w * h * 4) as usize {
                        frames.push(Frame { width: w, height: h, bgra: slot.bgra, pixels, at: slot.at });
                    }
                }
                FAILED => {
                    log::warn!("a recording's frame couldn't be read back from the GPU");
                    slot.state.store(FREE, Ordering::Release);
                }
                _ => {}
            }
        }
        frames.sort_by_key(|f| f.at);
        frames
    }

    /// Every frame still on its way back, waited for: as a recording ends.
    pub fn take_last(&mut self, device: &wgpu::Device) -> Vec<Frame> {
        if self.waiting() {
            let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(2)) });
        }
        self.take(device)
    }

    /// Whether a frame is still on its way back.
    pub fn waiting(&self) -> bool {
        self.slots.iter().any(|s| s.copied || matches!(s.state.load(Ordering::Acquire), WAITING | READY | FAILED))
    }

    /// What the buffers hold on the GPU.
    pub fn gpu_bytes(&self) -> u64 {
        self.slots.iter().filter(|s| s.buffer.is_some()).map(|s| s.row as u64 * s.size.1 as u64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crop_is_kept_inside_the_window() {
        let crop = Crop { x: 10, y: 20, w: 500, h: 400 };
        assert_eq!(crop.within(300, 300), Some(Crop { x: 10, y: 20, w: 290, h: 280 }));
        assert_eq!(crop.within(1000, 1000), Some(crop));
        assert_eq!(crop.within(10, 1000), None);
    }
}
