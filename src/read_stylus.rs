
use evdev::{AbsoluteAxisType, Device, InputEventKind};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub enum StylusEvent {
    Absolute { axis: evdev::AbsoluteAxisType, value: i32 },
    Tilt { axis: evdev::AbsoluteAxisType, value: i32 },
    Pressure {value: i32 },
    Key { key: evdev::Key, value: i32 },
}

pub fn read_input(device_path: String, sender: Sender<StylusEvent>) {
    thread::spawn(move || {
        let mut device = Device::open(device_path).expect("Konnte Gerät nicht öffnen");

        loop {
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        let stylus_event = match event.kind() {
                            InputEventKind::AbsAxis(axis) => {
                                match axis {
                                    AbsoluteAxisType::ABS_X | AbsoluteAxisType::ABS_Y => StylusEvent::Absolute { axis, value: event.value() },
                                    AbsoluteAxisType::ABS_TILT_X | AbsoluteAxisType::ABS_TILT_Y => StylusEvent::Tilt { axis, value: event.value() },
                                    AbsoluteAxisType::ABS_PRESSURE => StylusEvent::Pressure { value: event.value() },
                                    _ => panic!("Unhandled event in read stylus: {:?}", event)
                                }
                            },
                            InputEventKind::Key(key) => StylusEvent::Key { key, value: event.value() },
                            InputEventKind::Synchronization(_) => continue,
                            _ => panic!("Anderes event: {:?}", event),
                        };
                        if sender.send(stylus_event).is_err() {
                            // Empfänger wurde geschlossen
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Fehler beim Abrufen der Ereignisse: {}", e);
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });
}

