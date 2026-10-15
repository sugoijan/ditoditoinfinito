//! Per-instance vertex data in as many buffers as a frame needs: a frame
//! is never cut at one buffer's worth (the device's `max_buffer_size`), a
//! draw is split at the buffers' seams instead.

use std::ops::Range;

/// Instances a buffer holds at most (5 MB of 80-byte instances).
const CHUNK: usize = 1 << 16;

/// Instances a first buffer holds; it grows by doubling to a chunk.
const FIRST: usize = 256;

pub(crate) struct InstanceBuffers {
    label: &'static str,
    stride: usize,
    /// Instances a buffer holds.
    chunk: usize,
    /// Each with its capacity in instances.
    buffers: Vec<(wgpu::Buffer, usize)>,
}

impl InstanceBuffers {
    pub(crate) fn new(
        device: &wgpu::Device,
        label: &'static str,
        stride: usize,
    ) -> InstanceBuffers {
        let most = (device.limits().max_buffer_size / stride as u64).max(1);
        InstanceBuffers {
            label,
            stride,
            chunk: CHUNK.min(most.min(usize::MAX as u64) as usize),
            buffers: Vec::new(),
        }
    }

    /// Writes `count` instances (`bytes`, `stride` each) from the first,
    /// making or growing buffers as needed.
    pub(crate) fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) {
        let count = bytes.len() / self.stride;
        for (n, start) in (0..count).step_by(self.chunk).enumerate() {
            let len = (count - start).min(self.chunk);
            let wanted = if n == 0 {
                len.next_power_of_two().clamp(FIRST, self.chunk)
            } else {
                self.chunk
            };
            if self.buffers.get(n).is_none_or(|(_, cap)| *cap < len) {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(self.label),
                    size: (wanted * self.stride) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                if n < self.buffers.len() {
                    self.buffers[n] = (buffer, wanted);
                } else {
                    self.buffers.push((buffer, wanted));
                }
            }
            let at = start * self.stride;
            queue.write_buffer(&self.buffers[n].0, 0, &bytes[at..at + len * self.stride]);
        }
    }

    /// Instances `range` (of those uploaded), as pieces within one buffer
    /// each: the buffer and the range in it.
    pub(crate) fn pieces(
        &self,
        range: Range<u32>,
    ) -> impl Iterator<Item = (&wgpu::Buffer, Range<u32>)> {
        split(range, self.chunk).filter_map(|(n, r)| Some((&self.buffers.get(n)?.0, r)))
    }
}

/// `range` cut at multiples of `chunk`: the chunk's number and the range
/// within it.
fn split(range: Range<u32>, chunk: usize) -> impl Iterator<Item = (usize, Range<u32>)> {
    let chunk = chunk.max(1) as u32;
    let mut at = range.start;
    std::iter::from_fn(move || {
        if at >= range.end {
            return None;
        }
        let n = at / chunk;
        let end = range.end.min((n + 1) * chunk);
        let piece = (n as usize, at - n * chunk..end - n * chunk);
        at = end;
        Some(piece)
    })
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn ranges_are_cut_at_the_buffers_seams() {
        let pieces: Vec<_> = split(5..23, 8).collect();
        assert_eq!(pieces, vec![(0, 5..8), (1, 0..8), (2, 0..7)]);
        assert_eq!(split(8..16, 8).collect::<Vec<_>>(), vec![(1, 0..8)]);
        assert_eq!(split(3..3, 8).count(), 0);
    }
}
