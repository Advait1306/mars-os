use std::process::Command;

pub struct SystemState {
    pub volume: f32,
    pub muted: bool,
    pub brightness: Option<f32>,
    pub time_str: String,
}

impl SystemState {
    pub fn poll() -> Self {
        let (volume, muted) = poll_volume();
        Self {
            volume,
            muted,
            brightness: poll_brightness(),
            time_str: poll_time(),
        }
    }
}

pub fn poll_volume() -> (f32, bool) {
    let output = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output();
    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            let level = s
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let muted = s.contains("[MUTED]");
            (level, muted)
        }
        Err(_) => (0.0, false),
    }
}

pub fn poll_brightness() -> Option<f32> {
    let base = "/sys/class/backlight";
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            let cur = std::fs::read_to_string(path.join("brightness")).ok()?;
            let max = std::fs::read_to_string(path.join("max_brightness")).ok()?;
            let cur: f32 = cur.trim().parse().ok()?;
            let max: f32 = max.trim().parse().ok()?;
            if max > 0.0 {
                return Some((cur / max).clamp(0.0, 1.0));
            }
        }
    }
    let output = Command::new("brightnessctl")
        .args(["info", "-m"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = s.trim().split(',').collect();
    if parts.len() >= 4 {
        parts[3]
            .trim_end_matches('%')
            .parse::<f32>()
            .ok()
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
    } else {
        None
    }
}

pub fn poll_time() -> String {
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let hour = tm.tm_hour;
        let min = tm.tm_min;
        let (h12, ampm) = match hour {
            0 => (12, "AM"),
            1..=11 => (hour, "AM"),
            12 => (12, "PM"),
            _ => (hour - 12, "PM"),
        };
        format!("{}:{:02} {}", h12, min, ampm)
    }
}

pub fn set_volume(level: f32) {
    let pct = format!("{}%", (level * 100.0).round() as i32);
    let _ = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &pct])
        .spawn();
}

pub fn adjust_volume(delta: f32) {
    let pct = format!(
        "{}%{}",
        (delta.abs() * 100.0) as i32,
        if delta > 0.0 { "+" } else { "-" }
    );
    let _ = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &pct, "-l", "1.0"])
        .spawn();
}

pub fn toggle_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .spawn();
}

pub fn adjust_brightness(delta: f32) {
    let pct = format!(
        "{}%{}",
        (delta.abs() * 100.0) as i32,
        if delta > 0.0 { "+" } else { "-" }
    );
    let _ = Command::new("brightnessctl")
        .args(["set", &pct])
        .spawn();
}
