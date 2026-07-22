use std::process::Command;
use std::time::{Duration, SystemTime};

const MARKER: &str = "RINERTIA_AW";

const KWIN_SCRIPT_TEMPLATE: &str = r#"var w = workspace.activeWindow;
print("RINERTIA_AW {NONCE} " + (w ? w.resourceClass : "null"));
"#;

/// Detect the app class (resourceClass / WM_CLASS) of the focused window.
///
/// Best-effort with graceful degradation: KDE/KWin script bridge first,
/// then X11 via xprop, else None. All mechanisms are shell-outs; any failure
/// simply yields None (fail-open: momentum is never suppressed by accident).
pub fn detect() -> Option<String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if (desktop.contains("kde") || desktop.contains("plasma"))
        && let Some(class) = detect_kwin()
    {
        return Some(class);
    }
    if std::env::var_os("DISPLAY").is_some() {
        return detect_x11();
    }
    None
}

/// One-shot KWin script bridge: write a script with a unique nonce, load it
/// via org.kde.kwin.Scripting, run it (synchronous), and grep its print()
/// output back out of the user journal. KWin exposes no read API for the
/// active window, and scripts cannot own D-Bus names, so the journal is the
/// only zero-dependency channel back.
fn detect_kwin() -> Option<String> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let nonce = format!("{}_{}", std::process::id(), nanos);
    let script_path = format!("{}/rinertia-focused-app-{}.js", runtime_dir, nonce);

    std::fs::write(
        &script_path,
        KWIN_SCRIPT_TEMPLATE.replace("{NONCE}", &nonce),
    )
    .ok()?;
    let result = kwin_load_run_read(&script_path, &nonce);
    let _ = std::fs::remove_file(&script_path);
    result
}

fn kwin_load_run_read(script_path: &str, nonce: &str) -> Option<String> {
    let out = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
            "loadScript",
            "s",
            script_path,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let id = stdout.split_whitespace().nth(1)?;
    let obj = format!("/Scripting/Script{}", id);

    let run_ok = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            &obj,
            "org.kde.kwin.Script",
            "run",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let result = if run_ok {
        read_journal_marker(nonce)
    } else {
        None
    };

    let _ = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
            "unloadScript",
            "s",
            script_path,
        ])
        .output();

    result
}

fn read_journal_marker(nonce: &str) -> Option<String> {
    let marker = format!("{} {} ", MARKER, nonce);
    for _ in 0..5 {
        let out = Command::new("journalctl")
            .args([
                "--user",
                "_COMM=kwin_wayland",
                "--since",
                "-5s",
                "-o",
                "cat",
                "--no-pager",
            ])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines().rev() {
            if let Some(rest) = line.strip_prefix(&marker) {
                let class = rest.trim();
                return if class.is_empty() || class == "null" {
                    None
                } else {
                    Some(class.to_string())
                };
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    None
}

fn detect_x11() -> Option<String> {
    let out = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let id = stdout.trim().rsplit(' ').next()?;
    if id == "0x0" {
        return None;
    }

    let out = Command::new("xprop")
        .args(["-id", id, "WM_CLASS"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // WM_CLASS(STRING) = "instance", "Class"
    let class = stdout.trim().rsplit('"').nth(1)?;
    Some(class.to_string())
}

/// Case-insensitive substring match of `class` against the exclude patterns.
pub fn is_excluded(class: &str, exclude_apps: &[String]) -> bool {
    let class = class.to_lowercase();
    exclude_apps
        .iter()
        .any(|p| class.contains(&p.to_lowercase()))
}
