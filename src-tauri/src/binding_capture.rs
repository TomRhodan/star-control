use tauri::{AppHandle, Emitter};
use gilrs::{Gilrs, Event, EventType, Button, Axis, GamepadId};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use once_cell::sync::Lazy;
use chrono::Local;
use serde::{Deserialize, Serialize};

static IS_CAPTURING: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

fn log_capture(msg: &str) {
    let now = Local::now();
    log::debug!("[CAPTURE {}] {}", now.format("%H:%M:%S%.3f"), msg);
}

/// Convert Gilrs UUID (16 bytes) to hex string
fn uuid_to_hex(uuid: [u8; 16]) -> String {
    uuid.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Connected device info for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedDevice {
    /// Linux UUID from Gilrs - unique per physical device
    pub linux_uuid: String,
    /// Human-readable product name
    pub product_name: String,
    /// Device type: "joystick", "gamepad", "keyboard", "mouse"
    pub device_type: String,
    /// Current joystick instance (js1, js2, etc.) - may change on reconnect
    pub instance: u32,
}

/// Captured input event with full device info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedInput {
    /// Linux UUID from Gilrs - primary device identifier
    pub linux_uuid: String,
    /// Human-readable product name
    pub product_name: String,
    /// Device type
    pub device_type: String,
    /// Current joystick instance
    pub instance: u32,
    /// Input identifier (button5, x, z, etc.)
    pub input: String,
    /// Input type: "button", "axis", "hat"
    pub input_type: String,
}

/// List all connected gamepad/joystick devices
#[tauri::command]
pub fn list_connected_devices() -> Result<Vec<ConnectedDevice>, String> {
    let gilrs = Gilrs::new().map_err(|e| e.to_string())?;

    let mut devices = Vec::new();

    for (id, gamepad) in gilrs.gamepads() {
        let js_id: usize = id.into();
        let instance = (js_id + 1) as u32;

        devices.push(ConnectedDevice {
            linux_uuid: uuid_to_hex(gamepad.uuid()),
            product_name: gamepad.name().to_string(),
            device_type: "joystick".to_string(),
            instance,
        });
    }

    log_capture(&format!("Found {} connected devices", devices.len()));
    Ok(devices)
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

                // Build a map of GamepadId -> DeviceInfo for quick lookup
                let mut device_info_map: std::collections::HashMap<GamepadId, (String, String, u32)> = std::collections::HashMap::new();
                for (id, gamepad) in gilrs.gamepads() {
                    let js_id: usize = id.into();
                    let instance = (js_id + 1) as u32;
                    device_info_map.insert(id, (
                        uuid_to_hex(gamepad.uuid()),
                        gamepad.name().to_string(),
                        instance,
                    ));
                }

                while capturing.load(Ordering::SeqCst) {
                    while let Some(Event { id, event, .. }) = gilrs.next_event() {
                        // Get device info - if not in map, try to get it now
                        let (linux_uuid, product_name, instance) = if let Some(info) = device_info_map.get(&id) {
                            info.clone()
                        } else {
                            // Device might have been added since we started
                            // gilrs.gamepad(id) returns Gamepad directly in newer versions
                            let gamepad = gilrs.gamepad(id);
                            let js_id: usize = id.into();
                            let inst = (js_id + 1) as u32;
                            let info = (uuid_to_hex(gamepad.uuid()), gamepad.name().to_string(), inst);
                            device_info_map.insert(id, info.clone());
                            info
                        };

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
                                Some((btn_name, "button".to_string()))
                            }
                            EventType::AxisChanged(axis, val, code) if val.abs() > 0.8 => { // Threshold 0.8
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
                                Some((axis_name, "axis".to_string()))
                            }
                            _ => None
                        };

                        if let Some((input, input_type)) = sc_input {
                            // Create full input string with instance
                            let full_input = format!("js{}_{}", instance, input);

                            log_capture(&format!(
                                "CAPTURED: {} - {} ({}) -> {}",
                                product_name, input, input_type, full_input
                            ));

                            // Emit full CapturedInput structure
                            let captured = CapturedInput {
                                linux_uuid,
                                product_name,
                                device_type: "joystick".to_string(),
                                instance,
                                input: full_input,
                                input_type,
                            };

                            let _ = app_clone.emit("input-captured", captured);
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
