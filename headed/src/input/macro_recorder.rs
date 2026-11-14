//! Keyboard macro recording and playback.
//!
//! U1-06: Record KeyEvent sequences with timestamps and replay them.

use crate::input::{Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A serializable key identifier for macro storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializedKey {
    Character(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Unknown,
}

impl From<&Key> for SerializedKey {
    fn from(key: &Key) -> Self {
        match key {
            Key::Character(c) => SerializedKey::Character(*c),
            Key::Escape => SerializedKey::Escape,
            Key::CtrlBracket => SerializedKey::Escape, // Map to Escape for simplicity
            Key::Enter => SerializedKey::Enter,
            Key::Backspace => SerializedKey::Backspace,
            Key::Tab => SerializedKey::Tab,
            Key::Up => SerializedKey::Up,
            Key::Down => SerializedKey::Down,
            Key::Left => SerializedKey::Left,
            Key::Right => SerializedKey::Right,
            Key::Home => SerializedKey::Home,
            Key::End => SerializedKey::End,
            Key::PageUp => SerializedKey::PageUp,
            Key::PageDown => SerializedKey::PageDown,
            Key::F(n) => SerializedKey::F(*n),
            Key::Unknown => SerializedKey::Unknown,
        }
    }
}

impl From<SerializedKey> for Key {
    fn from(key: SerializedKey) -> Self {
        match key {
            SerializedKey::Character(c) => Key::Character(c),
            SerializedKey::Escape => Key::Escape,
            SerializedKey::Enter => Key::Enter,
            SerializedKey::Backspace => Key::Backspace,
            SerializedKey::Tab => Key::Tab,
            SerializedKey::Up => Key::Up,
            SerializedKey::Down => Key::Down,
            SerializedKey::Left => Key::Left,
            SerializedKey::Right => Key::Right,
            SerializedKey::Home => Key::Home,
            SerializedKey::End => Key::End,
            SerializedKey::PageUp => Key::PageUp,
            SerializedKey::PageDown => Key::PageDown,
            SerializedKey::F(n) => Key::F(n),
            SerializedKey::Unknown => Key::Unknown,
        }
    }
}

/// A serializable modifier state for macro storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl SerializedModifiers {
    pub fn none() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
        }
    }
}

impl From<&Modifiers> for SerializedModifiers {
    fn from(mods: &Modifiers) -> Self {
        Self {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            super_key: mods.super_key,
        }
    }
}

impl From<SerializedModifiers> for Modifiers {
    fn from(mods: SerializedModifiers) -> Self {
        Self {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            super_key: mods.super_key,
        }
    }
}

/// A recorded key event with relative timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Relative delay from the previous event (in milliseconds).
    pub delay_ms: u64,
    /// The key identifier.
    pub key: SerializedKey,
    /// Modifier state.
    pub modifiers: SerializedModifiers,
}

/// A saved macro containing a sequence of recorded events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMacro {
    /// Name of the macro.
    pub name: String,
    /// The recorded events.
    pub events: Vec<RecordedEvent>,
    /// When the macro was recorded.
    pub recorded_at: String,
}

/// State of macro recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    /// Not recording.
    Idle,
    /// Currently recording key events.
    Recording,
}

/// Macro recorder that captures key events with timestamps.
pub struct MacroRecorder {
    /// Current recording state.
    state: RecordingState,
    /// Events recorded in the current session.
    events: Vec<RecordedEvent>,
    /// Timestamp of the last recorded event.
    last_event_time: Option<Instant>,
    /// Saved macros keyed by name.
    saved_macros: HashMap<String, SavedMacro>,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
            events: Vec::new(),
            last_event_time: None,
            saved_macros: HashMap::new(),
        }
    }

    /// Start recording macro events.
    pub fn start_recording(&mut self) {
        self.state = RecordingState::Recording;
        self.events.clear();
        self.last_event_time = Some(Instant::now());
    }

    /// Stop recording and return the recorded events.
    pub fn stop_recording(&mut self) -> Vec<RecordedEvent> {
        self.state = RecordingState::Idle;
        self.last_event_time = None;
        std::mem::take(&mut self.events)
    }

    /// Record a key event if currently recording.
    pub fn record_event(&mut self, key: Key, modifiers: Modifiers) {
        if self.state != RecordingState::Recording {
            return;
        }

        let now = Instant::now();
        let delay_ms = self
            .last_event_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        self.events.push(RecordedEvent {
            delay_ms,
            key: SerializedKey::from(&key),
            modifiers: SerializedModifiers::from(&modifiers),
        });
        self.last_event_time = Some(now);
    }

    /// Check if currently recording.
    pub fn is_recording(&self) -> bool {
        self.state == RecordingState::Recording
    }

    /// Save a macro with the given name.
    pub fn save_macro(&mut self, name: &str, events: Vec<RecordedEvent>) {
        let saved = SavedMacro {
            name: name.to_string(),
            events,
            recorded_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        self.saved_macros.insert(name.to_string(), saved);
    }

    /// Get a saved macro by name.
    pub fn get_macro(&self, name: &str) -> Option<&SavedMacro> {
        self.saved_macros.get(name)
    }

    /// List all saved macro names.
    pub fn list_macros(&self) -> Vec<&str> {
        self.saved_macros.keys().map(|s| s.as_str()).collect()
    }

    /// Delete a saved macro.
    pub fn delete_macro(&mut self, name: &str) -> bool {
        self.saved_macros.remove(name).is_some()
    }

    /// Get the number of events in the current recording.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

impl Default for MacroRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro player that replays events with original timing.
pub struct MacroPlayer {
    /// Events to replay.
    events: Vec<RecordedEvent>,
    /// Current index in the replay sequence.
    index: usize,
    /// Whether playback is active.
    playing: bool,
}

impl MacroPlayer {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            index: 0,
            playing: false,
        }
    }

    /// Start playing a macro.
    pub fn start_playback(&mut self, events: Vec<RecordedEvent>) {
        self.events = events;
        self.index = 0;
        self.playing = !self.events.is_empty();
    }

    /// Get the next event to play, with its delay.
    /// Returns None when playback is complete.
    pub fn next_event(&mut self) -> Option<(RecordedEvent, Duration)> {
        if !self.playing || self.index >= self.events.len() {
            self.playing = false;
            return None;
        }

        let event = self.events[self.index].clone();
        self.index += 1;

        // If this is the last event, mark playback as complete
        if self.index >= self.events.len() {
            self.playing = false;
        }

        let delay = Duration::from_millis(event.delay_ms);
        Some((event, delay))
    }

    /// Check if playback is active.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Stop playback.
    pub fn stop(&mut self) {
        self.playing = false;
        self.index = 0;
        self.events.clear();
    }

    /// Get the total number of events in the current playback.
    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    /// Get the current playback progress (0.0 to 1.0).
    pub fn progress(&self) -> f64 {
        if self.events.is_empty() {
            return 0.0;
        }
        self.index as f64 / self.events.len() as f64
    }
}

impl Default for MacroPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_recorded_event(key: char) -> RecordedEvent {
        RecordedEvent {
            delay_ms: 0,
            key: SerializedKey::Character(key),
            modifiers: SerializedModifiers {
                ctrl: false,
                alt: false,
                shift: false,
                super_key: false,
            },
        }
    }

    #[test]
    fn test_recorder_new() {
        let recorder = MacroRecorder::new();
        assert!(!recorder.is_recording());
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn test_start_stop_recording() {
        let mut recorder = MacroRecorder::new();
        recorder.start_recording();
        assert!(recorder.is_recording());

        let events = recorder.stop_recording();
        assert!(!recorder.is_recording());
        assert!(events.is_empty());
    }

    #[test]
    fn test_record_events() {
        let mut recorder = MacroRecorder::new();
        recorder.start_recording();

        recorder.record_event(Key::Character('a'), Modifiers::none());
        recorder.record_event(Key::Character('b'), Modifiers::ctrl());

        assert_eq!(recorder.event_count(), 2);

        let events = recorder.stop_recording();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].delay_ms, 0); // First event has 0 delay
    }

    #[test]
    fn test_save_load_macro() {
        let mut recorder = MacroRecorder::new();
        recorder.start_recording();
        recorder.record_event(Key::Character('a'), Modifiers::none());
        let events = recorder.stop_recording();

        recorder.save_macro("test", events);
        assert_eq!(recorder.list_macros().len(), 1);
        assert!(recorder.get_macro("test").is_some());
    }

    #[test]
    fn test_delete_macro() {
        let mut recorder = MacroRecorder::new();
        recorder.save_macro("test", vec![]);
        assert!(recorder.delete_macro("test"));
        assert!(!recorder.delete_macro("test"));
    }

    #[test]
    fn test_player_new() {
        let player = MacroPlayer::new();
        assert!(!player.is_playing());
        assert_eq!(player.total_events(), 0);
    }

    #[test]
    fn test_player_playback() {
        let mut player = MacroPlayer::new();
        let events = vec![
            RecordedEvent {
                delay_ms: 0,
                key: SerializedKey::Character('a'),
                modifiers: SerializedModifiers::none(),
            },
            RecordedEvent {
                delay_ms: 100,
                key: SerializedKey::Character('b'),
                modifiers: SerializedModifiers::none(),
            },
        ];

        player.start_playback(events);
        assert!(player.is_playing());

        let (event, delay) = player.next_event().unwrap();
        assert_eq!(event.key, SerializedKey::Character('a'));
        assert_eq!(delay, Duration::from_millis(0));

        let (event, delay) = player.next_event().unwrap();
        assert_eq!(event.key, SerializedKey::Character('b'));
        assert_eq!(delay, Duration::from_millis(100));

        assert!(player.next_event().is_none());
        assert!(!player.is_playing());
    }

    #[test]
    fn test_player_empty_playback() {
        let mut player = MacroPlayer::new();
        player.start_playback(vec![]);
        assert!(!player.is_playing());
        assert!(player.next_event().is_none());
    }

    #[test]
    fn test_player_progress() {
        let mut player = MacroPlayer::new();
        let events = vec![
            make_recorded_event('a'),
            make_recorded_event('b'),
            make_recorded_event('c'),
        ];

        player.start_playback(events);
        assert_eq!(player.progress(), 0.0);

        player.next_event();
        assert!((player.progress() - 0.333).abs() < 0.01);

        player.next_event();
        assert!((player.progress() - 0.666).abs() < 0.01);

        player.next_event();
        assert_eq!(player.progress(), 1.0);
    }

    #[test]
    fn test_player_stop() {
        let mut player = MacroPlayer::new();
        player.start_playback(vec![make_recorded_event('a')]);
        player.stop();
        assert!(!player.is_playing());
        assert!(player.next_event().is_none());
    }

    #[test]
    fn test_serialized_key_conversion() {
        let key = Key::Character('a');
        let serialized = SerializedKey::from(&key);
        let restored: Key = serialized.into();
        assert_eq!(key, restored);
    }

    #[test]
    fn test_serialized_modifiers_conversion() {
        let mods = Modifiers::ctrl();
        let serialized = SerializedModifiers::from(&mods);
        let restored: Modifiers = serialized.into();
        assert_eq!(mods.ctrl, restored.ctrl);
    }
}
