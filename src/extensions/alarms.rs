//! WebExtensions `browser.alarms` API.
//!
//! Provides timed callbacks for extensions. Extensions create named alarms
//! with a delay and optional period. When an alarm fires, registered callbacks
//! are invoked with the alarm name and scheduled time.

use std::sync::Arc;

use crate::extensions::types::Result;

/// A named alarm with its scheduled fire time and optional period.
#[derive(Debug, Clone)]
pub struct AlarmInfo {
    /// Name of the alarm. Defaults to "" if not provided.
    pub name: String,
    /// When the alarm should fire (seconds since epoch).
    pub scheduled_time: f64,
    /// If set, the alarm repeats with this period in minutes.
    pub period_in_minutes: Option<f64>,
}

/// Parameters for creating a new alarm.
#[derive(Debug, Clone)]
pub struct AlarmCreateParams {
    /// Optional name; defaults to "" (empty string).
    pub name: Option<String>,
    /// Time at which the alarm should fire, in milliseconds since the epoch.
    /// Mutually exclusive with `delay_in_minutes`.
    pub when: Option<f64>,
    /// Delay from now in minutes. Mutually exclusive with `when`.
    pub delay_in_minutes: Option<f64>,
    /// If set, the alarm repeats with this period in minutes after the first fire.
    pub period_in_minutes: Option<f64>,
}

/// Extension alarms API — timed callbacks for background tasks.
pub trait AlarmsApi: Send + Sync {
    /// Create an alarm. Replaces any existing alarm with the same name.
    fn create(&self, params: AlarmCreateParams) -> Result<()>;

    /// Get information about a specific alarm.
    fn get(&self, name: Option<&str>) -> Result<Option<AlarmInfo>>;

    /// Get all active alarms for this extension.
    fn get_all(&self) -> Result<Vec<AlarmInfo>>;

    /// Clear a specific alarm by name. Returns true if the alarm existed.
    fn clear(&self, name: Option<&str>) -> Result<bool>;

    /// Clear all alarms. Returns true if any alarms were cleared.
    fn clear_all(&self) -> Result<bool>;

    /// Register a callback for when any alarm fires.
    fn on_alarm(&self, callback: Arc<dyn Fn(AlarmInfo) + Send + Sync>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alarm_info_fields() {
        let info = AlarmInfo {
            name: "test".into(),
            scheduled_time: 1000.0,
            period_in_minutes: Some(5.0),
        };
        assert_eq!(info.name, "test");
        assert_eq!(info.scheduled_time, 1000.0);
        assert_eq!(info.period_in_minutes, Some(5.0));
    }

    #[test]
    fn test_alarm_create_params_defaults() {
        let params = AlarmCreateParams {
            name: None,
            when: None,
            delay_in_minutes: Some(1.0),
            period_in_minutes: None,
        };
        assert!(params.name.is_none());
        assert!(params.when.is_none());
        assert_eq!(params.delay_in_minutes, Some(1.0));
    }

    #[test]
    fn test_alarm_create_params_with_when() {
        let params = AlarmCreateParams {
            name: Some("my-alarm".into()),
            when: Some(1700000000000.0),
            delay_in_minutes: None,
            period_in_minutes: Some(10.0),
        };
        assert_eq!(params.name.as_deref(), Some("my-alarm"));
        assert_eq!(params.when, Some(1700000000000.0));
        assert_eq!(params.period_in_minutes, Some(10.0));
    }
}
