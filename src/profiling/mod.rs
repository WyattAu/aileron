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

const FRAME_BUDGET_MS: f64 = 16.7;
const RECOVERY_THRESHOLD_MS: f64 = 14.0;
const MIN_QUALITY: f32 = 0.2;
const MAX_QUALITY: f32 = 1.0;
const QUALITY_REDUCTION_STEP: f32 = 0.2;
const QUALITY_RECOVERY_STEP: f32 = 0.1;
const BASE_CAPTURE_INTERVAL_MS: u32 = 33;

pub struct AdaptiveQuality {
    quality_level: f32,
    over_budget_count: u32,
    reduction_threshold: u32,
    recovery_threshold: u32,
    under_budget_count: u32,
    enabled: bool,
}

impl AdaptiveQuality {
    pub fn new() -> Self {
        Self {
            quality_level: MAX_QUALITY,
            over_budget_count: 0,
            reduction_threshold: 5,
            recovery_threshold: 30,
            under_budget_count: 0,
            enabled: true,
        }
    }

    pub fn update(&mut self, frame_time_ms: f64) {
        if !self.enabled {
            return;
        }

        if frame_time_ms > FRAME_BUDGET_MS {
            self.over_budget_count += 1;
            self.under_budget_count = 0;
            if self.over_budget_count >= self.reduction_threshold {
                self.quality_level = (self.quality_level - QUALITY_REDUCTION_STEP).max(MIN_QUALITY);
                self.over_budget_count = 0;
            }
        } else if frame_time_ms < RECOVERY_THRESHOLD_MS {
            self.under_budget_count += 1;
            self.over_budget_count = 0;
            if self.under_budget_count >= self.recovery_threshold {
                self.quality_level = (self.quality_level + QUALITY_RECOVERY_STEP).min(MAX_QUALITY);
                self.under_budget_count = 0;
            }
        } else {
            self.over_budget_count = 0;
            self.under_budget_count = 0;
        }
    }

    pub fn quality_level(&self) -> f32 {
        self.quality_level
    }

    pub fn should_skip_capture(&self, _pane_index: usize, _active_pane_index: usize) -> bool {
        if !self.enabled {
            return false;
        }
        false
    }

    pub fn should_skip_non_active(&self) -> bool {
        self.enabled && self.quality_level < 0.6
    }

    pub fn capture_interval_ms(&self) -> u32 {
        if !self.enabled {
            return BASE_CAPTURE_INTERVAL_MS;
        }
        (BASE_CAPTURE_INTERVAL_MS as f32 / self.quality_level).round() as u32
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.quality_level = MAX_QUALITY;
            self.over_budget_count = 0;
            self.under_budget_count = 0;
        }
    }
}

impl Default for AdaptiveQuality {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_adaptive_quality_default() {
        let aq = AdaptiveQuality::new();
        assert_eq!(aq.quality_level(), 1.0);
        assert!(aq.enabled());
        assert_eq!(aq.capture_interval_ms(), 33);
    }

    #[test]
    fn test_quality_reduction_triggers_after_threshold() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..5 {
            aq.update(20.0);
        }
        assert_eq!(aq.quality_level(), 0.8);
    }

    #[test]
    fn test_quality_recovery_triggers_after_threshold() {
        let mut aq = AdaptiveQuality::new();
        aq.update(20.0);
        aq.update(20.0);
        aq.update(20.0);
        aq.update(20.0);
        aq.update(20.0);
        assert_eq!(aq.quality_level(), 0.8);
        for _ in 0..30 {
            aq.update(10.0);
        }
        assert!((aq.quality_level() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_active_pane_never_skipped() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..10 {
            aq.update(20.0);
        }
        assert!(!aq.should_skip_capture(0, 0));
    }

    #[test]
    fn test_non_active_panes_skipped_at_low_quality() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..15 {
            aq.update(20.0);
        }
        assert!(aq.quality_level() < 0.6);
        assert!(aq.should_skip_non_active());
    }

    #[test]
    fn test_min_quality_bound() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..50 {
            aq.update(20.0);
        }
        assert!(aq.quality_level() >= MIN_QUALITY);
    }

    #[test]
    fn test_max_quality_bound() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..100 {
            aq.update(10.0);
        }
        assert!(aq.quality_level() <= MAX_QUALITY);
    }

    #[test]
    fn test_disabled_state() {
        let mut aq = AdaptiveQuality::new();
        aq.set_enabled(false);
        assert!(!aq.enabled());
        assert!(!aq.should_skip_capture(1, 0));
        assert!(!aq.should_skip_non_active());
        assert_eq!(aq.capture_interval_ms(), 33);
        aq.update(20.0);
        assert_eq!(aq.quality_level(), 1.0);
    }

    #[test]
    fn test_reenable_resets_quality() {
        let mut aq = AdaptiveQuality::new();
        for _ in 0..10 {
            aq.update(20.0);
        }
        assert!(aq.quality_level() < 1.0);
        aq.set_enabled(false);
        aq.set_enabled(true);
        assert_eq!(aq.quality_level(), 1.0);
    }

    #[test]
    fn test_in_between_frame_time_resets_counters() {
        let mut aq = AdaptiveQuality::new();
        aq.update(20.0);
        aq.update(20.0);
        aq.update(15.0);
        aq.update(20.0);
        aq.update(20.0);
        aq.update(20.0);
        assert_eq!(aq.quality_level(), 1.0);
    }

    #[test]
    fn test_capture_interval_scales_with_quality() {
        let mut aq = AdaptiveQuality::new();
        assert_eq!(aq.capture_interval_ms(), 33);
        for _ in 0..5 {
            aq.update(20.0);
        }
        assert_eq!(aq.quality_level(), 0.8);
        assert_eq!(aq.capture_interval_ms(), 41);
    }

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
