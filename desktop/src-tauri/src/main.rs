// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Panics must reach a file on every platform (GUI processes have no stderr
    // on Windows, and the tauri log plugin owns the global logger later). The
    // hook appends directly to the app's log dir.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("[{}] panic: {info}", chrono_now_compact());
        eprintln!("{msg}");
        if let Some(dir) = panic_log_dir() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&dir) {
                let _ = writeln!(f, "{msg}");
            }
        }
    }));

    if buzz_lib::print_agent_access_owner_only_probe_if_requested() {
        return;
    }

    // Before anything else: WebKitGTK reads its rendering environment once at
    // process start, and this is the only point where the process is still
    // single threaded and no GTK object exists yet, which is what makes
    // `std::env::set_var` sound.
    #[cfg(target_os = "linux")]
    if let Err(diagnostic) = buzz_lib::webkit_rendering::apply() {
        eprintln!("buzz-desktop: {diagnostic}");
        std::process::exit(1);
    }

    buzz_lib::run()
}

fn panic_log_dir() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|h| {
            std::path::PathBuf::from(h).join(".local/share")
        })
    }?;
    Some(base.join("app.native.omni").join("logs"))
}

fn chrono_now_compact() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
