pub mod memory;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct FrameSample {
    pub timestamp: Instant,
    pub duration: Duration,
    pub phase: String,
}

pub struct Profiler {
    samples: VecDeque<FrameSample>,
    max_samples: usize,
    enabled: bool,
    frame_start: Option<Instant>,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            max_samples: 1000,
            enabled: false,
            frame_start: None,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.samples.clear();
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        if self.enabled {
            self.samples.clear();
        }
        self.enabled
    }

    pub fn start_frame(&mut self) {
        if self.enabled {
            self.frame_start = Some(Instant::now());
        }
    }

    pub fn end_frame(&mut self, phase: &str) {
        if let Some(start) = self.frame_start.take() {
            let duration = start.elapsed();
            self.samples.push_back(FrameSample {
                timestamp: start,
                duration,
                phase: phase.to_string(),
            });
            while self.samples.len() > self.max_samples {
                self.samples.pop_front();
            }
        }
    }

    pub fn stats(&self) -> FrameStats {
        if self.samples.is_empty() {
            return FrameStats::default();
        }
        let mut durations: Vec<f64> = self
            .samples
            .iter()
            .map(|s| s.duration.as_secs_f64() * 1000.0)
            .collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

        FrameStats {
            count: durations.len(),
            min_ms: durations.first().copied().unwrap_or(0.0),
            p50_ms: durations.get(durations.len() / 2).copied().unwrap_or(0.0),
            p95_ms: durations
                .get((durations.len() as f64 * 0.95) as usize)
                .copied()
                .unwrap_or(0.0),
            p99_ms: durations
                .get((durations.len() as f64 * 0.99) as usize)
                .copied()
                .unwrap_or(0.0),
            max_ms: durations.last().copied().unwrap_or(0.0),
            avg_ms: durations.iter().sum::<f64>() / durations.len() as f64,
            dropped_frames: durations.iter().filter(|d| **d > 16.7).count(),
        }
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub count: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub dropped_frames: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_profiler_new() {
        let p = Profiler::new();
        assert!(!p.is_enabled());
        assert_eq!(p.sample_count(), 0);
    }

    #[test]
    fn test_enable_disable_toggle() {
        let mut p = Profiler::new();
        assert!(!p.is_enabled());

        p.enable();
        assert!(p.is_enabled());

        p.disable();
        assert!(!p.is_enabled());

        let on = p.toggle();
        assert!(on);
        assert!(p.is_enabled());

        let off = p.toggle();
        assert!(!off);
        assert!(!p.is_enabled());
    }

    #[test]
    fn test_toggle_clears_samples() {
        let mut p = Profiler::new();
        p.enable();
        p.start_frame();
        thread::sleep(Duration::from_micros(100));
        p.end_frame("test");
        assert_eq!(p.sample_count(), 1);

        p.disable();
        p.toggle();
        assert!(p.is_enabled());
        assert_eq!(p.sample_count(), 0);
    }

    #[test]
    fn test_frame_recording() {
        let mut p = Profiler::new();
        p.enable();
        p.start_frame();
        thread::sleep(Duration::from_millis(2));
        p.end_frame("render");
        assert_eq!(p.sample_count(), 1);
    }

    #[test]
    fn test_frame_ignored_when_disabled() {
        let mut p = Profiler::new();
        p.start_frame();
        p.end_frame("render");
        assert_eq!(p.sample_count(), 0);
    }

    #[test]
    fn test_max_samples() {
        let mut p = Profiler::new();
        p.max_samples = 5;
        p.enable();
        for _ in 0..10 {
            p.start_frame();
            p.end_frame("render");
        }
        assert_eq!(p.sample_count(), 5);
    }

    #[test]
    fn test_clear() {
        let mut p = Profiler::new();
        p.enable();
        p.start_frame();
        p.end_frame("test");
        p.start_frame();
        p.end_frame("test");
        assert_eq!(p.sample_count(), 2);
        p.clear();
        assert_eq!(p.sample_count(), 0);
    }

    #[test]
    fn test_stats_empty() {
        let p = Profiler::new();
        let s = p.stats();
        assert_eq!(s.count, 0);
        assert_eq!(s.avg_ms, 0.0);
        assert_eq!(s.dropped_frames, 0);
    }

    #[test]
    fn test_stats_single_sample() {
        let mut p = Profiler::new();
        p.enable();
        p.start_frame();
        thread::sleep(Duration::from_millis(5));
        p.end_frame("render");
        let s = p.stats();
        assert_eq!(s.count, 1);
        assert!(s.avg_ms > 0.0);
        assert!(s.min_ms > 0.0);
        assert_eq!(s.min_ms, s.max_ms);
        assert_eq!(s.p50_ms, s.min_ms);
    }

    #[test]
    fn test_stats_percentiles() {
        let mut p = Profiler::new();
        p.enable();
        for _ in 0..100 {
            p.start_frame();
            thread::sleep(Duration::from_micros(10));
            p.end_frame("render");
        }
        let s = p.stats();
        assert_eq!(s.count, 100);
        assert!(s.p50_ms <= s.p95_ms);
        assert!(s.p95_ms <= s.p99_ms);
        assert!(s.p99_ms <= s.max_ms);
        assert!(s.min_ms <= s.avg_ms);
    }

    #[test]
    fn test_dropped_frames_count() {
        let mut p = Profiler::new();
        p.enable();
        for _ in 0..5 {
            p.start_frame();
            thread::sleep(Duration::from_millis(1));
            p.end_frame("fast");
        }
        let s = p.stats();
        assert_eq!(s.dropped_frames, 0);

        p.start_frame();
        thread::sleep(Duration::from_millis(20));
        p.end_frame("slow");
        let s = p.stats();
        assert!(s.dropped_frames >= 1);
    }
}
