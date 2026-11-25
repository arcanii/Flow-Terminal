//! How long the GPU spends on a frame, where the adapter can say
//! (`wgpu::Features::TIMESTAMP_QUERY`): while About's Resources or a tab's
//! card shows, each pass of a frame writes the GPU's clock as it begins and
//! as it ends, and the spans, read back a frame or so later without waiting
//! for them, add up to the frame's time on the GPU. Passes rather than the
//! whole of a frame's commands, since Apple's GPUs take the time only at a
//! pass's edges; and the gaps between passes, where the GPU may be idle,
//! don't count.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The timestamps a frame may write, two a pass: the liquid's steps are
/// most of a frame's passes, a few dozen.
const QUERIES: u32 = 256;
/// The readings the time shown is the mean of: those of the last second.
const SPAN: Duration = Duration::from_secs(1);

/// A frame's times as they're read back: still on their way, read, or lost.
const WAITING: u8 = 0;
const READ: u8 = 1;
const LOST: u8 = 2;

pub struct GpuClock {
    /// None where the adapter has no timestamps.
    queries: Option<Queries>,
    /// Frames are timed while what shows their time shows.
    on: bool,
    /// The frame being encoded is timed, and the timestamps it has handed
    /// out to its passes so far.
    timing: Cell<bool>,
    next: Cell<u32>,
    /// Each timed frame's milliseconds on the GPU, and when it was read.
    readings: VecDeque<(Instant, f32)>,
}

struct Queries {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Nanoseconds a tick of the GPU's clock.
    period: f32,
    /// The timestamps of the frame last timed, in `read` until they're
    /// read back: no frame is timed meanwhile.
    pending: Option<u32>,
    state: Arc<AtomicU8>,
}

impl GpuClock {
    /// A clock for `device`, which times nothing where it has no
    /// timestamps (Flow asks for them where the adapter has them).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let queries = device.features().contains(wgpu::Features::TIMESTAMP_QUERY).then(|| {
            let buffer = |label, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: QUERIES as u64 * 8, usage, mapped_at_creation: false });
            Queries {
                set: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("frame times"), ty: wgpu::QueryType::Timestamp, count: QUERIES }),
                resolve: buffer("frame times", wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
                read: buffer("frame times read", wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
                period: queue.get_timestamp_period(),
                pending: None,
                state: Arc::new(AtomicU8::new(WAITING)),
            }
        });
        Self::with(queries)
    }

    /// A clock that never times anything.
    #[cfg(test)]
    pub fn off() -> Self {
        Self::with(None)
    }

    fn with(queries: Option<Queries>) -> Self {
        Self { queries, on: false, timing: Cell::new(false), next: Cell::new(0), readings: VecDeque::new() }
    }

    /// Whether this GPU can say how long it takes.
    pub fn measures(&self) -> bool {
        self.queries.is_some()
    }

    /// Times frames from now on, or stops, forgetting what it read.
    pub fn set_on(&mut self, on: bool) {
        if self.on && !on {
            self.readings.clear();
        }
        self.on = on;
    }

    /// The mean of the last second's frames on the GPU, in milliseconds:
    /// none before the first is read, or while frames aren't timed.
    pub fn frame_ms(&self) -> Option<f32> {
        let n = self.readings.len();
        (n > 0).then(|| self.readings.iter().map(|(_, ms)| ms).sum::<f32>() / n as f32)
    }

    /// A frame is about to be encoded: it's timed while frames are, unless
    /// the last one timed hasn't been read back yet. Its times wait in
    /// the GPU's buffers for `poll`, called first.
    pub fn begin_frame(&self) {
        self.next.set(0);
        self.timing.set(self.on && self.queries.as_ref().is_some_and(|q| q.pending.is_none()));
    }

    /// Where a pass begun now writes its times: none while the frame isn't
    /// timed, or once it has handed out all it may.
    pub fn writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let q = self.queries.as_ref().filter(|_| self.timing.get())?;
        let at = self.next.get();
        if at + 2 > QUERIES {
            return None;
        }
        self.next.set(at + 2);
        Some(wgpu::RenderPassTimestampWrites { query_set: &q.set, beginning_of_pass_write_index: Some(at), end_of_pass_write_index: Some(at + 1) })
    }

    /// The frame's commands are all encoded: what its passes wrote goes
    /// into the buffer read back.
    pub fn end_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let written = self.next.get();
        let Some(q) = self.queries.as_mut().filter(|_| self.timing.get() && written > 0) else { return };
        encoder.resolve_query_set(&q.set, 0..written, &q.resolve, 0);
        encoder.copy_buffer_to_buffer(&q.resolve, 0, &q.read, 0, written as u64 * 8);
        q.pending = Some(written);
    }

    /// The frame was submitted: its times are asked for, to read once the
    /// GPU has done.
    pub fn submitted(&mut self) {
        let timed = self.timing.replace(false);
        let Some(q) = self.queries.as_mut() else { return };
        let Some(written) = q.pending.filter(|_| timed) else { return };
        q.state.store(WAITING, Ordering::Release);
        let state = q.state.clone();
        q.read.map_async(wgpu::MapMode::Read, ..written as u64 * 8, move |done| {
            state.store(if done.is_ok() { READ } else { LOST }, Ordering::Release);
        });
    }

    /// Reads the times of the frame last timed if the GPU has done with it,
    /// without waiting for it.
    pub fn poll(&mut self, device: &wgpu::Device) {
        let Some(q) = self.queries.as_mut() else { return };
        let Some(written) = q.pending else { return };
        let _ = device.poll(wgpu::PollType::Poll);
        match q.state.load(Ordering::Acquire) {
            WAITING => return,
            READ => {
                let ticks: Vec<u64> = match q.read.get_mapped_range(..written as u64 * 8) {
                    Ok(data) => data.as_chunks::<8>().0.iter().map(|b| u64::from_le_bytes(*b)).collect(),
                    Err(_) => Vec::new(),
                };
                q.read.unmap();
                let now = Instant::now();
                if self.on && !ticks.is_empty() {
                    self.readings.push_back((now, (spans(&ticks) as f64 * q.period as f64 / 1e6) as f32));
                }
                while self.readings.front().is_some_and(|(at, _)| now.duration_since(*at) > SPAN) {
                    self.readings.pop_front();
                }
            }
            _ => {}
        }
        q.pending = None;
    }
}

/// The ticks between each pass's two timestamps, added up: a pair that
/// goes backwards (a pass the GPU didn't time, its zeros) counts nothing.
fn spans(ticks: &[u64]) -> u64 {
    ticks.as_chunks::<2>().0.iter().map(|[begun, ended]| ended.saturating_sub(*begun)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frames_time_on_the_gpu_is_its_passes_spans_added_up() {
        // Two passes, with a gap between them that doesn't count, and one
        // the GPU didn't time.
        assert_eq!(spans(&[100, 160, 200, 230, 0, 0]), 90);
        assert_eq!(spans(&[500, 400]), 0);
        assert_eq!(spans(&[]), 0);
    }

    #[test]
    fn a_clock_off_or_without_timestamps_hands_out_nothing_and_reads_nothing() {
        let mut clock = GpuClock::off();
        clock.set_on(true);
        clock.begin_frame();
        assert!(clock.writes().is_none());
        assert!(!clock.measures());
        assert_eq!(clock.frame_ms(), None);
    }
}
