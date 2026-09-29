pub const TILE: usize = 128;

#[derive(Clone)]
pub struct DirtyTile {
    pub index: usize,
    pub previous: Option<u64>,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub pixels: Vec<u8>,
}

pub struct Plan {
    pub width: u16,
    pub height: u16,
    pub hashes: Vec<Option<u64>>,
    pub dirty: Vec<DirtyTile>,
}

#[derive(Default)]
pub struct DiffState {
    width: u16,
    height: u16,
    hashes: Vec<Option<u64>>,
}

impl DiffState {
    pub fn plan(
        &self,
        frame: &[u8],
        width: u32,
        height: u32,
        stride: usize,
    ) -> Result<Plan, &'static str> {
        let width = u16::try_from(width).map_err(|_| "frame is too wide")?;
        let height = u16::try_from(height).map_err(|_| "frame is too tall")?;
        if width == 0 || height == 0 {
            return Err("empty frame");
        }
        let row = width as usize * 4;
        if stride < row {
            return Err("frame stride is too short");
        }
        let need = (height as usize)
            .checked_mul(stride)
            .ok_or("frame is too large")?;
        if frame.len() < need {
            return Err("frame is shorter than its stride");
        }
        let cols = (width as usize).div_ceil(TILE);
        let rows = (height as usize).div_ceil(TILE);
        let fresh =
            self.width != width || self.height != height || self.hashes.len() != cols * rows;
        let mut hashes = vec![None; cols * rows];
        let mut dirty = Vec::new();
        for row_index in 0..rows {
            for col in 0..cols {
                let x = col * TILE;
                let y = row_index * TILE;
                let w = (width as usize - x).min(TILE);
                let h = (height as usize - y).min(TILE);
                let hash = hash_rect(frame, stride, x, y, w, h);
                let index = row_index * cols + col;
                let previous = if fresh { None } else { self.hashes[index] };
                hashes[index] = Some(hash);
                if previous == Some(hash) {
                    continue;
                }
                let mut pixels = vec![0u8; w * h * 4];
                for line in 0..h {
                    let src = (y + line) * stride + x * 4;
                    let dst = line * w * 4;
                    pixels[dst..dst + w * 4].copy_from_slice(&frame[src..src + w * 4]);
                }
                dirty.push(DirtyTile {
                    index,
                    previous,
                    x: x as u16,
                    y: y as u16,
                    w: w as u16,
                    h: h as u16,
                    pixels,
                });
            }
        }
        Ok(Plan {
            width,
            height,
            hashes,
            dirty,
        })
    }

    pub fn commit(&mut self, width: u16, height: u16, hashes: Vec<Option<u64>>) {
        self.width = width;
        self.height = height;
        self.hashes = hashes;
    }
}

pub fn geometry_message(width: u16, height: u16) -> [u8; 5] {
    let mut out = [0u8; 5];
    out[0] = 1;
    out[1..3].copy_from_slice(&width.to_be_bytes());
    out[3..5].copy_from_slice(&height.to_be_bytes());
    out
}

pub fn tile_message(x: u16, y: u16, jpeg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + jpeg.len());
    out.push(2);
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&y.to_be_bytes());
    out.extend_from_slice(jpeg);
    out
}

fn hash_rect(frame: &[u8], stride: usize, x: usize, y: usize, w: usize, h: usize) -> u64 {
    let mut hash = 0x9e3779b97f4a7c15u64;
    for row in 0..h {
        let start = (y + row) * stride + x * 4;
        let bytes = &frame[start..start + w * 4];
        let mut chunks = bytes.chunks_exact(8);
        for chunk in chunks.by_ref() {
            let word = u64::from_le_bytes([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
            ]);
            hash = hash.rotate_left(5) ^ word.wrapping_mul(0x9e3779b97f4a7c15);
        }
        if !chunks.remainder().is_empty() {
            let mut word = 0u64;
            for (shift, &byte) in chunks.remainder().iter().enumerate() {
                word |= u64::from(byte) << (shift * 8);
            }
            hash = hash.rotate_left(5) ^ word.wrapping_mul(0x9e3779b97f4a7c15);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_tiles_are_returned() {
        let width = 130u32;
        let height = 10u32;
        let stride = 130 * 4 + 16;
        let mut frame = vec![0u8; stride * height as usize];
        let mut diff = DiffState::default();
        let first = diff.plan(&frame, width, height, stride).unwrap();
        assert_eq!(first.dirty.len(), 2);
        assert_eq!(first.dirty[1].x, 128);
        assert_eq!(first.dirty[1].w, 2);
        diff.commit(first.width, first.height, first.hashes);
        frame[130 * 4] = 9;
        assert!(diff
            .plan(&frame, width, height, stride)
            .unwrap()
            .dirty
            .is_empty());
        frame[128 * 4] = 1;
        let changed = diff.plan(&frame, width, height, stride).unwrap();
        assert_eq!(changed.dirty.len(), 1);
        assert_eq!(changed.dirty[0].x, 128);
        assert_eq!(changed.dirty[0].pixels[0], 1);
    }

    #[test]
    fn messages_use_the_viewer_layout() {
        let geometry = geometry_message(1920, 1080);
        assert_eq!(geometry[0], 1);
        assert_eq!(u16::from_be_bytes(geometry[1..3].try_into().unwrap()), 1920);
        assert_eq!(u16::from_be_bytes(geometry[3..5].try_into().unwrap()), 1080);
        let tile = tile_message(8, 16, &[0xff, 0xd8]);
        assert_eq!(tile[0], 2);
        assert_eq!(u16::from_be_bytes(tile[1..3].try_into().unwrap()), 8);
        assert_eq!(u16::from_be_bytes(tile[3..5].try_into().unwrap()), 16);
        assert_eq!(&tile[5..], &[0xff, 0xd8]);
    }
}
