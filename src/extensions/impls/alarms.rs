//! Concrete implementation of [`AlarmsApi`] for Aileron.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::extensions::alarms::{AlarmCreateParams, AlarmInfo, AlarmsApi};
use crate::extensions::types::{ExtensionError, ListenerId, Result};

type AlarmCallback = Arc<dyn Fn(AlarmInfo) + Send + Sync>;

pub struct AileronAlarmsApi {
    alarms: RwLock<HashMap<String, AlarmInfo>>,
    callbacks: RwLock<Vec<(ListenerId, AlarmCallback)>>,
}

impl Default for AileronAlarmsApi {
    fn default() -> Self {
        Self::new()
    }
}

impl AileronAlarmsApi {
    pub fn new() -> Self {
        Self {
            alarms: RwLock::new(HashMap::new()),
            callbacks: RwLock::new(Vec::new()),
        }
    }

    /// Fire all callbacks for alarms whose scheduled_time has passed.
    /// Called periodically by the sync execution loop or frame tick.
    pub fn fire_due_alarms(&self, now_ms: f64) {
        let mut to_fire = Vec::new();
        let mut alarms = self.alarms.write();

        for (_name, alarm) in alarms.iter_mut() {
            if alarm.scheduled_time <= now_ms {
                to_fire.push(alarm.clone());

                if let Some(period) = alarm.period_in_minutes {
                    // Reschedule: period is in minutes, scheduled_time is ms
                    alarm.scheduled_time += period * 60_000.0;
                } else {
                    // One-shot: mark for removal by name
                }
            }
        }

        // Remove one-shot alarms that fired
        alarms
            .retain(|_, alarm| alarm.scheduled_time > now_ms || alarm.period_in_minutes.is_some());

        drop(alarms);

        // Fire callbacks outside write lock
        let callbacks = self.callbacks.read();
        for alarm_info in to_fire {
            for (_, cb) in callbacks.iter() {
                cb(alarm_info.clone());
            }
        }
    }
}

fn compute_scheduled_time(params: &AlarmCreateParams) -> Result<f64> {
    match (params.when, params.delay_in_minutes) {
        (Some(when_ms), _) => Ok(when_ms),
        (_, Some(delay)) => {
            let now_ms = now_ms();
            Ok(now_ms + delay * 60_000.0)
        }
        (None, None) => Err(ExtensionError::InvalidArgument(
            "Either 'when' or 'delayInMinutes' must be provided".into(),
        )),
    }
}

fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

impl AlarmsApi for AileronAlarmsApi {
    fn create(&self, params: AlarmCreateParams) -> Result<()> {
        let name = params.name.clone().unwrap_or_default();
        let scheduled_time = compute_scheduled_time(&params)?;

        let alarm = AlarmInfo {
            name: name.clone(),
            scheduled_time,
            period_in_minutes: params.period_in_minutes,
        };

        let mut alarms = self.alarms.write();
        alarms.insert(name, alarm);
        Ok(())
    }

    fn get(&self, name: Option<&str>) -> Result<Option<AlarmInfo>> {
        let alarms = self.alarms.read();
        let key = name.unwrap_or("");
        Ok(alarms.get(key).cloned())
    }

    fn get_all(&self) -> Result<Vec<AlarmInfo>> {
        let alarms = self.alarms.read();
        Ok(alarms.values().cloned().collect())
    }

    fn clear(&self, name: Option<&str>) -> Result<bool> {
        let mut alarms = self.alarms.write();
        let key = name.unwrap_or("");
        Ok(alarms.remove(key).is_some())
    }

    fn clear_all(&self) -> Result<bool> {
        let mut alarms = self.alarms.write();
        let count = alarms.len();
        alarms.clear();
        Ok(count > 0)
    }

    fn on_alarm(&self, callback: Arc<dyn Fn(AlarmInfo) + Send + Sync>) {
        let mut callbacks = self.callbacks.write();
        let id = ListenerId(super::super::impls::next_listener_id_raw());
        callbacks.push((id, callback));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_alarm() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: Some("test".into()),
            when: Some(9999999999999.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        let alarm = api.get(Some("test")).unwrap();
        assert!(alarm.is_some());
        let alarm = alarm.unwrap();
        assert_eq!(alarm.name, "test");
        assert_eq!(alarm.scheduled_time, 9999999999999.0);
        assert!(alarm.period_in_minutes.is_none());
    }

    #[test]
    fn test_create_replaces_existing() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: Some("dup".into()),
            when: Some(1000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();
        api.create(AlarmCreateParams {
            name: Some("dup".into()),
            when: Some(2000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        let alarm = api.get(Some("dup")).unwrap().unwrap();
        assert_eq!(alarm.scheduled_time, 2000.0);
    }

    #[test]
    fn test_get_default_name() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: None,
            when: Some(5000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        let alarm = api.get(None).unwrap();
        assert!(alarm.is_some());
        assert_eq!(alarm.unwrap().name, "");
    }

    #[test]
    fn test_get_all() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: Some("a".into()),
            when: Some(1000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();
        api.create(AlarmCreateParams {
            name: Some("b".into()),
            when: Some(2000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        let all = api.get_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_clear_alarm() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: Some("rm".into()),
            when: Some(1000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        assert!(api.clear(Some("rm")).unwrap());
        assert!(!api.clear(Some("rm")).unwrap());
        assert!(api.get(Some("rm")).unwrap().is_none());
    }

    #[test]
    fn test_clear_all() {
        let api = AileronAlarmsApi::new();
        api.create(AlarmCreateParams {
            name: Some("a".into()),
            when: Some(1000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();
        api.create(AlarmCreateParams {
            name: Some("b".into()),
            when: Some(2000.0),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        assert!(api.clear_all().unwrap());
        assert!(api.get_all().unwrap().is_empty());
        assert!(!api.clear_all().unwrap());
    }

    #[test]
    fn test_create_with_delay() {
        let api = AileronAlarmsApi::new();
        let before = now_ms();
        api.create(AlarmCreateParams {
            name: Some("delayed".into()),
            when: None,
            delay_in_minutes: Some(5.0),
            period_in_minutes: None,
        })
        .unwrap();

        let alarm = api.get(Some("delayed")).unwrap().unwrap();
        // Should be ~5 minutes from now (300000ms)
        assert!(alarm.scheduled_time >= before + 299_000.0);
        assert!(alarm.scheduled_time <= before + 301_000.0);
    }

    #[test]
    fn test_create_missing_when_and_delay() {
        let api = AileronAlarmsApi::new();
        let result = api.create(AlarmCreateParams {
            name: Some("bad".into()),
            when: None,
            delay_in_minutes: None,
            period_in_minutes: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_fire_due_alarms_one_shot() {
        let api = AileronAlarmsApi::new();
        let past = now_ms() - 10_000.0; // 10 seconds ago
        api.create(AlarmCreateParams {
            name: Some("past".into()),
            when: Some(past),
            delay_in_minutes: None,
            period_in_minutes: None,
        })
        .unwrap();

        let fired = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fired_clone = fired.clone();
        api.on_alarm(Arc::new(move |_info| {
            fired_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));

        api.fire_due_alarms(now_ms());
        assert_eq!(fired.load(std::sync::atomic::Ordering::Relaxed), 1);

        // One-shot alarm should be removed
        assert!(api.get(Some("past")).unwrap().is_none());
    }

    #[test]
    fn test_fire_due_alarms_periodic_reschedules() {
        let api = AileronAlarmsApi::new();
        let past = now_ms() - 10_000.0;
        api.create(AlarmCreateParams {
            name: Some("periodic".into()),
            when: Some(past),
            delay_in_minutes: None,
            period_in_minutes: Some(1.0),
        })
        .unwrap();

        let fired = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let fired_clone = fired.clone();
        api.on_alarm(Arc::new(move |_info| {
            fired_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));

        api.fire_due_alarms(now_ms());
        assert_eq!(fired.load(std::sync::atomic::Ordering::Relaxed), 1);

        // Periodic alarm should still exist with rescheduled time
        let alarm = api.get(Some("periodic")).unwrap().unwrap();
        assert!(alarm.scheduled_time > past);
        assert_eq!(alarm.period_in_minutes, Some(1.0));
    }

    #[test]
    fn test_periodic_alarm_preserved() {
        let api = AileronAlarmsApi::new();
        let future = now_ms() + 600_000.0; // 10 minutes from now
        api.create(AlarmCreateParams {
            name: Some("future".into()),
            when: Some(future),
            delay_in_minutes: None,
            period_in_minutes: Some(5.0),
        })
        .unwrap();

        api.fire_due_alarms(now_ms());
        // Should not fire, should still exist
        assert!(api.get(Some("future")).unwrap().is_some());
    }
}
