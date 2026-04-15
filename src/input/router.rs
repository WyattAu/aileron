use crate::input::mode::{Key, KeyEvent, Mode};

/// Where an input event should be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDestination {
    /// Forward to Servo (web content) for typing/interaction
    Servo,
    /// Forward to egui for UI interaction
    Egui,
    /// Forward to command palette for search/command input
    CommandPalette,
    /// Execute a bound action via the keybinding handler
    KeybindingHandler,
    /// Discard the event
    Discard,
}

/// Route an input event to the correct subsystem based on current mode.
/// Implements DEF-MODE-003 from YP-INPUT-MODES-001.
pub fn route_event(mode: Mode, event: &KeyEvent) -> EventDestination {
    match mode {
        Mode::Normal => {
            if is_mouse_or_window_event(event) {
                EventDestination::Egui
            } else if is_input_event(event) {
                EventDestination::KeybindingHandler
            } else {
                EventDestination::Discard
            }
        }
        Mode::Insert => {
            if is_mouse_or_window_event(event) {
                EventDestination::Egui
            } else if is_input_event(event) {
                EventDestination::Servo
            } else {
                EventDestination::Discard
            }
        }
        Mode::Command => {
            if is_mouse_or_window_event(event) {
                EventDestination::Egui
            } else if is_input_event(event) {
                EventDestination::CommandPalette
            } else {
                EventDestination::Discard
            }
        }
    }
}

fn is_input_event(event: &KeyEvent) -> bool {
    matches!(
        event.key,
        Key::Character(_) | Key::Escape | Key::Enter | Key::Backspace | Key::Tab
    )
}

fn is_mouse_or_window_event(event: &KeyEvent) -> bool {
    matches!(
        event.key,
        Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::F(_)
    ) || event.modifiers.ctrl
        || event.modifiers.alt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    fn key_event(key: Key) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::none(),
            physical_key: None,
        }
    }

    fn key_event_with_modifiers(key: Key, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            key,
            modifiers,
            physical_key: None,
        }
    }

    #[test]
    fn test_normal_mode_routes_input_to_keybinding() {
        assert_eq!(
            route_event(Mode::Normal, &key_event(Key::Character('j'))),
            EventDestination::KeybindingHandler
        );
        assert_eq!(
            route_event(Mode::Normal, &key_event(Key::Character('k'))),
            EventDestination::KeybindingHandler
        );
    }

    #[test]
    fn test_insert_mode_routes_input_to_servo() {
        assert_eq!(
            route_event(Mode::Insert, &key_event(Key::Character('h'))),
            EventDestination::Servo
        );
        assert_eq!(
            route_event(Mode::Insert, &key_event(Key::Character('e'))),
            EventDestination::Servo
        );
    }

    #[test]
    fn test_insert_mode_does_not_route_to_servo() {
        assert_ne!(
            route_event(Mode::Normal, &key_event(Key::Character('h'))),
            EventDestination::Servo
        );
    }

    #[test]
    fn test_command_mode_routes_to_palette() {
        assert_eq!(
            route_event(Mode::Command, &key_event(Key::Character('q'))),
            EventDestination::CommandPalette
        );
    }

    #[test]
    fn test_mouse_events_go_to_egui() {
        for mode in [Mode::Normal, Mode::Insert, Mode::Command] {
            assert_eq!(
                route_event(mode, &key_event(Key::Up)),
                EventDestination::Egui
            );
            assert_eq!(
                route_event(mode, &key_event(Key::Down)),
                EventDestination::Egui
            );
            assert_eq!(
                route_event(mode, &key_event(Key::Left)),
                EventDestination::Egui
            );
            assert_eq!(
                route_event(mode, &key_event(Key::Right)),
                EventDestination::Egui
            );
        }
    }

    #[test]
    fn test_ctrl_combos_go_to_egui() {
        let ctrl = Modifiers::ctrl();
        for mode in [Mode::Normal, Mode::Insert, Mode::Command] {
            assert_eq!(
                route_event(mode, &key_event_with_modifiers(Key::Character('e'), ctrl)),
                EventDestination::Egui
            );
        }
    }

    #[test]
    fn test_escape_routes_to_keybinding_in_normal() {
        // In Normal mode, Escape is a keybinding (no-op typically)
        assert_eq!(
            route_event(Mode::Normal, &key_event(Key::Escape)),
            EventDestination::KeybindingHandler
        );
    }

    #[test]
    fn test_every_event_has_destination() {
        // Property: route_event is total and returns a destination for any input
        let keys = vec![
            Key::Character('a'),
            Key::Character('z'),
            Key::Character('0'),
            Key::Character('9'),
            Key::Escape,
            Key::Enter,
            Key::Backspace,
            Key::Tab,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::F(1),
            Key::F(12),
            Key::Unknown,
        ];
        let modes = [Mode::Normal, Mode::Insert, Mode::Command];
        for mode in modes {
            for key in &keys {
                let event = key_event(key.clone());
                // Should not panic and should always return a valid destination
                let _dest = route_event(mode, &event);
            }
        }
    }
}
