//! The player is a GUI executable; only authoring/CLI paths attach a console.
use std::ffi::OsString;

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Published,
    Engineering,
    Command,
}
fn mode(args: &[OsString], adjacent_package: bool) -> Mode {
    if args
        .iter()
        .any(|a| a == "--check" || a == "--help" || a == "-h")
    {
        Mode::Command
    } else if args.iter().any(|a| a == "--project")
        || (!adjacent_package && !args.iter().any(|a| a == "--package"))
    {
        Mode::Engineering
    } else {
        Mode::Published
    }
}
/// Returns whether an interactive startup error needs a native dialog.
pub fn initialize() -> bool {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let adjacent = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("game.kpak")))
        .is_some_and(|p| p.is_file());
    let mode = mode(&args, adjacent);
    #[cfg(windows)]
    if mode != Mode::Published {
        attach(mode == Mode::Engineering);
    }
    mode == Mode::Published
}
#[cfg(windows)]
fn attach(allocate: bool) {
    use windows::Win32::{
        Storage::FileSystem::{FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType},
        System::Console::*,
    };
    // AttachConsole may replace std handles. Keep inherited redirection intact.
    unsafe {
        let redirected = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|kind| {
            let handle = GetStdHandle(kind).ok().filter(|h| !h.is_invalid());
            (
                kind,
                handle.filter(|h| matches!(GetFileType(*h), FILE_TYPE_DISK | FILE_TYPE_PIPE)),
            )
        });
        let attached = AttachConsole(ATTACH_PARENT_PROCESS).is_ok();
        if !attached
            && allocate
            && redirected.iter().all(|(_, h)| h.is_none())
            && GetConsoleWindow().is_invalid()
        {
            let _ = AllocConsole();
        }
        for (kind, handle) in redirected {
            if let Some(handle) = handle {
                let _ = SetStdHandle(kind, handle);
            }
        }
    }
}
pub fn error_dialog(message: &str) {
    #[cfg(windows)]
    unsafe {
        use windows::{
            Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            core::{PCWSTR, w},
        };
        let wide = message
            .replace('\0', "�")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            w!("Kakera 启动失败"),
            MB_OK | MB_ICONERROR,
        );
    }
    #[cfg(not(windows))]
    let _ = message;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn published_launch_never_requests_console_but_cli_and_explicit_project_do() {
        let args = |s: &[&str]| s.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(mode(&args(&[]), true), Mode::Published);
        assert_eq!(
            mode(&args(&["--package", "game.kpak"]), false),
            Mode::Published
        );
        assert_eq!(mode(&args(&["--renderer", "cpu"]), true), Mode::Published);
        assert_eq!(mode(&args(&["--project", "game"]), true), Mode::Engineering);
        assert_eq!(mode(&args(&[]), false), Mode::Engineering);
        assert_eq!(
            mode(&args(&["--package", "game.kpak", "--check"]), true),
            Mode::Command
        );
        assert_eq!(mode(&args(&["--help"]), true), Mode::Command);
    }
}
