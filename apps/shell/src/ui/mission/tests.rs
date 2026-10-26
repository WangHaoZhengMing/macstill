use super::{Gesture, gesture_wants_open};

#[test]
fn preparation_does_not_open_after_a_cancel_or_horizontal_gesture() {
    let mut gesture = Gesture {
        seq: 1,
        direction: "up".into(),
        progress: 0.01,
        active: true,
        cancelled: false,
    };
    assert!(gesture_wants_open(Some(&gesture)));
    gesture.active = false;
    assert!(!gesture_wants_open(Some(&gesture)));
    gesture.progress = 0.7;
    assert!(gesture_wants_open(Some(&gesture)));
    gesture.cancelled = true;
    assert!(!gesture_wants_open(Some(&gesture)));
    gesture.active = true;
    gesture.direction = "down".into();
    assert!(!gesture_wants_open(Some(&gesture)));
    assert!(!gesture_wants_open(None));
}
