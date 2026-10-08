use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use screencapturekit::cm::SCFrameStatus;
use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

use crate::system::active_display_ids;

pub const TILE_COLUMNS: usize = 32;
pub const TILE_ROWS: usize = 18;
const TILE_COUNT: usize = TILE_COLUMNS * TILE_ROWS;
const TILE_LUMA_DELTA: f32 = 3.0;
const CAPTURE_WIDTH: u32 = 320;

pub fn tile_luminance(width: usize, height: usize, bytes_per_row: usize, bgra: &[u8]) -> Vec<f32> {
    let mut sums = vec![0f32; TILE_COUNT];
    let mut counts = vec![0u32; TILE_COUNT];
    if width == 0 || height == 0 {
        return sums;
    }
    for y in 0..height {
        let row = y * bytes_per_row;
        let ty = (y * TILE_ROWS / height).min(TILE_ROWS - 1);
        for x in 0..width {
            let i = row + x * 4;
            if i + 2 >= bgra.len() {
                break;
            }
            let luma = 0.0722 * f32::from(bgra[i])
                + 0.7152 * f32::from(bgra[i + 1])
                + 0.2126 * f32::from(bgra[i + 2]);
            let tx = (x * TILE_COLUMNS / width).min(TILE_COLUMNS - 1);
            let tile = ty * TILE_COLUMNS + tx;
            sums[tile] += luma;
            counts[tile] += 1;
        }
    }
    for (sum, count) in sums.iter_mut().zip(counts) {
        if count > 0 {
            *sum /= count as f32;
        }
    }
    sums
}

fn dirty_ratio(width: usize, height: usize, rects: &[(f64, f64, f64, f64)]) -> f64 {
    let space_w = rects.iter().map(|r| r.0 + r.2).fold(width as f64, f64::max);
    let space_h = rects
        .iter()
        .map(|r| r.1 + r.3)
        .fold(height as f64, f64::max);
    let area: f64 = rects.iter().map(|r| r.2.max(0.0) * r.3.max(0.0)).sum();
    (area / (space_w * space_h).max(1.0)).min(1.0)
}

#[derive(Default)]
pub struct MotionFrameAnalyzer {
    previous: HashMap<u32, Vec<f32>>,
}

impl MotionFrameAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn analyze(
        &mut self,
        display: u32,
        width: usize,
        height: usize,
        bytes_per_row: usize,
        bgra: &[u8],
        dirty_rects: Option<&[(f64, f64, f64, f64)]>,
    ) -> f64 {
        let tiles = tile_luminance(width, height, bytes_per_row, bgra);
        let tile_ratio = match self.previous.insert(display, tiles.clone()) {
            Some(prev) if prev.len() == tiles.len() => {
                let changed = prev
                    .iter()
                    .zip(&tiles)
                    .filter(|(a, b)| (*a - *b).abs() >= TILE_LUMA_DELTA)
                    .count();
                changed as f64 / TILE_COUNT as f64
            }
            _ => 0.0,
        };
        match dirty_rects {
            Some(rects) => dirty_ratio(width, height, rects).min(tile_ratio),
            None => tile_ratio,
        }
    }

    pub fn reset(&mut self) {
        self.previous.clear();
    }
}

#[derive(Default)]
struct Shared {
    analyzer: MotionFrameAnalyzer,
    max_ratio: f64,
    frames: u64,
}

pub struct ScreenMotionProbe {
    shared: Arc<Mutex<Shared>>,
    streams: Vec<SCStream>,
    displays: Vec<u32>,
    excluded_bundle_ids: Vec<String>,
}

impl ScreenMotionProbe {
    pub fn new(excluded_bundle_ids: Vec<String>) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            streams: Vec::new(),
            displays: Vec::new(),
            excluded_bundle_ids,
        }
    }

    pub fn is_running(&self) -> bool {
        !self.streams.is_empty()
    }

    pub fn start(&mut self) -> Result<(), String> {
        self.stop();
        let content = SCShareableContent::get().map_err(|e| e.to_string())?;
        let excluded: Vec<SCRunningApplication> = content
            .applications()
            .into_iter()
            .filter(|app| self.excluded_bundle_ids.contains(&app.bundle_identifier()))
            .collect();
        let excluded_refs: Vec<&SCRunningApplication> = excluded.iter().collect();
        let mut displays = Vec::new();
        for display in content.displays() {
            let id = display.display_id();
            let (w, h) = scaled_size(display.width(), display.height());
            let filter = SCContentFilter::create()
                .with_display(&display)
                .with_excluding_applications(&excluded_refs, &[])
                .build()
                .map_err(|e| e.to_string())?;
            let config = SCStreamConfiguration::new()
                .with_width(w)
                .with_height(h)
                .with_pixel_format(PixelFormat::BGRA)
                .with_shows_cursor(false)
                .with_fps(1)
                .with_queue_depth(3);
            let mut stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
            let shared = Arc::clone(&self.shared);
            stream
                .add_output_handler(
                    move |sample: CMSampleBuffer, kind: SCStreamOutputType| {
                        if matches!(kind, SCStreamOutputType::Screen) {
                            handle_frame(&shared, id, &sample);
                        }
                    },
                    SCStreamOutputType::Screen,
                )
                .map_err(|e| e.to_string())?;
            stream.start_capture().map_err(|e| e.to_string())?;
            self.streams.push(stream);
            displays.push(id);
        }
        displays.sort_unstable();
        self.displays = displays;
        Ok(())
    }

    pub fn stop(&mut self) {
        for stream in self.streams.drain(..) {
            let _ = stream.stop_capture();
        }
        if let Ok(mut shared) = self.shared.lock() {
            shared.analyzer.reset();
            shared.max_ratio = 0.0;
        }
        self.displays.clear();
    }

    pub fn displays_changed(&self) -> bool {
        self.is_running() && active_display_ids() != self.displays
    }

    pub fn take_changed_ratio(&self) -> f64 {
        self.shared
            .lock()
            .map(|mut s| std::mem::take(&mut s.max_ratio))
            .unwrap_or(0.0)
    }

    pub fn frames_received(&self) -> u64 {
        self.shared.lock().map(|s| s.frames).unwrap_or(0)
    }
}

impl Drop for ScreenMotionProbe {
    fn drop(&mut self) {
        self.stop();
    }
}

fn scaled_size(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (CAPTURE_WIDTH, CAPTURE_WIDTH * 9 / 16);
    }
    let w = width.min(CAPTURE_WIDTH);
    let h = ((u64::from(height) * u64::from(w)) / u64::from(width)).max(2) as u32;
    (w, h & !1)
}

fn handle_frame(shared: &Mutex<Shared>, display: u32, sample: &CMSampleBuffer) {
    if sample.frame_status() != Some(SCFrameStatus::Complete) {
        return;
    }
    let dirty: Option<Vec<(f64, f64, f64, f64)>> = sample.dirty_rects().map(|rects| {
        rects
            .iter()
            .map(|r| (r.origin.x, r.origin.y, r.size.width, r.size.height))
            .collect()
    });
    let Some(buffer) = sample.pixel_buffer() else {
        return;
    };
    let Ok(guard) = buffer.lock(CVPixelBufferLockFlags::READ_ONLY) else {
        return;
    };
    let Some(bytes) = (unsafe { guard.as_slice() }) else {
        return;
    };
    let Ok(mut shared) = shared.lock() else {
        return;
    };
    let ratio = shared.analyzer.analyze(
        display,
        guard.width(),
        guard.height(),
        guard.bytes_per_row(),
        bytes,
        dirty.as_deref(),
    );
    shared.max_ratio = shared.max_ratio.max(ratio);
    shared.frames += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, fill: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let mut data = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let v = fill(x, y);
                let i = (y * width + x) * 4;
                data[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        data
    }

    #[test]
    fn identical_frames_have_no_motion() {
        let mut analyzer = MotionFrameAnalyzer::new();
        let a = frame(320, 180, |x, _| (x % 256) as u8);
        assert_eq!(analyzer.analyze(1, 320, 180, 1280, &a, None), 0.0);
        assert_eq!(analyzer.analyze(1, 320, 180, 1280, &a, None), 0.0);
    }

    #[test]
    fn changed_quarter_of_screen_is_detected() {
        let mut analyzer = MotionFrameAnalyzer::new();
        let a = frame(320, 180, |_, _| 10);
        let b = frame(320, 180, |x, y| if x < 160 && y < 90 { 200 } else { 10 });
        analyzer.analyze(1, 320, 180, 1280, &a, None);
        let ratio = analyzer.analyze(1, 320, 180, 1280, &b, None);
        assert!((ratio - 0.25).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn dirty_rects_cap_the_ratio() {
        let mut analyzer = MotionFrameAnalyzer::new();
        let a = frame(320, 180, |_, _| 10);
        let b = frame(320, 180, |_, _| 90);
        analyzer.analyze(1, 320, 180, 1280, &a, None);
        let ratio = analyzer.analyze(1, 320, 180, 1280, &b, Some(&[(0.0, 0.0, 32.0, 18.0)]));
        assert!((ratio - 0.01).abs() < 0.001, "{ratio}");
    }

    #[test]
    fn displays_are_tracked_independently() {
        let mut analyzer = MotionFrameAnalyzer::new();
        let a = frame(64, 36, |_, _| 0);
        let b = frame(64, 36, |_, _| 255);
        analyzer.analyze(1, 64, 36, 256, &a, None);
        assert_eq!(analyzer.analyze(2, 64, 36, 256, &b, None), 0.0);
        assert_eq!(analyzer.analyze(1, 64, 36, 256, &b, None), 1.0);
    }

    #[test]
    fn scaled_size_keeps_aspect() {
        assert_eq!(scaled_size(3024, 1964), (320, 206));
        assert_eq!(scaled_size(1920, 1080), (320, 180));
    }
}
