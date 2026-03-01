use tauri::{AppHandle, Emitter};
use gilrs::{Gilrs, Event, EventType, Button, Axis};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use once_cell::sync::Lazy;
use chrono::Local;

static IS_CAPTURING: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

fn log_capture(msg: &str) {
    let now = Local::now();
    println!("[CAPTURE {}] {}", now.format("%H:%M:%S%.3f"), msg);
}

#[tauri::command]
pub fn start_input_capture(app: AppHandle) {
    if IS_CAPTURING.load(Ordering::SeqCst) { return; }
    IS_CAPTURING.store(true, Ordering::SeqCst);
    log_capture(">>> HARDWARE CAPTURE ENABLED <<<");

    let app_clone = app.clone();
    let capturing = IS_CAPTURING.clone();

    thread::spawn(move || {
        match Gilrs::new() {
            Ok(mut gilrs) => {
                log_capture(&format!("Gilrs active. Found {} devices.", gilrs.gamepads().count()));
                while capturing.load(Ordering::SeqCst) {
                    while let Some(Event { id, event, .. }) = gilrs.next_event() {
                        let js_id: usize = id.into();
                        let sc_input = match event {
                            EventType::ButtonPressed(button, code) => {
                                let btn_name = if button != Button::Unknown {
                                    format_gilrs_button(button)
                                } else {
                                    let code_str = format!("{:?}", code);
                                    if code_str.contains("code: 288") { "button1".to_string() }
                                    else if code_str.contains("code: 713") { "button26".to_string() }
                                    else if code_str.contains("code: 708") { "button21".to_string() }
                                    else {
                                        code_str.split("code: ")
                                            .nth(1)
                                            .and_then(|s| s.split(' ').next())
                                            .and_then(|s| s.parse::<u32>().ok())
                                            .map(|c| format!("button{}", if c >= 704 { c - 704 + 17 } else if c >= 288 { c - 288 + 1 } else { c }))
                                            .unwrap_or_else(|| "unknown".to_string())
                                    }
                                };
                                Some(format!("js{}_{}", js_id + 1, btn_name))
                            }
                            EventType::AxisChanged(axis, val, code) if val.abs() > 0.8 => { // Increased threshold to 0.8
                                let code_str = format!("{:?}", code);
                                let axis_name = match axis {
                                    Axis::LeftStickX => "x".to_string(),
                                    Axis::LeftStickY => "y".to_string(),
                                    Axis::LeftZ => "z".to_string(),
                                    Axis::RightStickX => "rotx".to_string(),
                                    Axis::RightStickY => "roty".to_string(),
                                    Axis::RightZ => "rotz".to_string(),
                                    _ => {
                                        if code_str.contains("code: 6") { "slider1".to_string() }
                                        else if code_str.contains("code: 7") { "slider2".to_string() }
                                        else { format!("{:?}", axis).to_lowercase() }
                                    }
                                };
                                Some(format!("js{}_{}", js_id + 1, axis_name))
                            }
                            _ => None
                        };

                        if let Some(input) = sc_input {
                            log_capture(&format!("MAPPED INPUT: {} (Source: {:?})", input, event));
                            let _ = app_clone.emit("input-captured", input);
                        }
                    }
                    thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            Err(e) => log_capture(&format!("ERROR: {:?}", e)),
        }
    });
}

#[tauri::command]
pub fn stop_input_capture() {
    IS_CAPTURING.store(false, Ordering::SeqCst);
    log_capture(">>> HARDWARE CAPTURE DISABLED <<<");
}

fn format_gilrs_button(btn: Button) -> String {
    match btn {
        Button::South => "button1".to_string(),
        Button::East => "button2".to_string(),
        Button::North => "button3".to_string(),
        Button::West => "button4".to_string(),
        Button::LeftTrigger => "button5".to_string(),
        Button::RightTrigger => "button6".to_string(),
        Button::Select => "button7".to_string(),
        Button::Start => "button8".to_string(),
        _ => format!("button{:?}", btn).to_lowercase(),
    }
}
