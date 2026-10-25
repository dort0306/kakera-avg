//! Client-area geometry shared by native sizing and programmatic window restoration.
pub const INITIAL: [u32; 2] = [1280, 720];

/// Exact physical pixels: choosing a multiple of 16×9 avoids fractional black borders.
pub fn size(width: u32, scale: f64) -> [u32; 2] {
    let minimum = (40.0 * scale).ceil().max(1.0) as u32;
    let units = ((u64::from(width) + 8) / 16).max(u64::from(minimum)) as u32;
    [units * 16, units * 9]
}

/// WM_SIZING edges are 1…8. Keep the opposite corner/edge anchored and add decorations.
pub fn sizing(rect: [i32; 4], border: [i32; 2], edge: usize, scale: f64) -> [i32; 4] {
    let [mut left, mut top, mut right, mut bottom] = rect;
    let width = (right - left - border[0]).max(1) as u32;
    let height = (bottom - top - border[1]).max(1) as u32;
    let width = if matches!(edge, 3 | 6) {
        (u64::from(height) * 16 / 9) as u32
    } else {
        width
    };
    let [width, height] = size(width, scale);
    let width = width as i32 + border[0];
    let height = height as i32 + border[1];
    if matches!(edge, 1 | 4 | 7) {
        left = right - width;
    } else {
        right = left + width;
    }
    if matches!(edge, 3..=5) {
        top = bottom - height;
    } else {
        bottom = top + height;
    }
    [left, top, right, bottom]
}

#[cfg(windows)]
pub mod native {
    use super::sizing;
    use windows::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                GetClientRect, GetWindowRect, SWP_NOSIZE, WINDOWPOS, WM_NCDESTROY, WM_SIZING,
                WM_WINDOWPOSCHANGING,
            },
        },
    };

    // The subclass is attached/detached on the owning UI thread and owns no borrowed state.
    pub fn install(hwnd: HWND) -> anyhow::Result<()> {
        unsafe {
            SetWindowSubclass(hwnd, Some(procedure), 0x4b414b45, 0).ok()?;
        }
        Ok(())
    }

    pub fn fullscreen(hwnd: HWND, enabled: bool) -> anyhow::Result<()> {
        unsafe {
            SetWindowSubclass(hwnd, Some(procedure), 0x4b414b45, usize::from(enabled)).ok()?;
        }
        Ok(())
    }

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        if msg == WM_WINDOWPOSCHANGING && data == 0 && lp.0 != 0 {
            let position = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
            if !position.flags.contains(SWP_NOSIZE) && position.cx > 0 && position.cy > 0 {
                let mut outer = RECT::default();
                let mut client = RECT::default();
                if unsafe { GetWindowRect(hwnd, &mut outer) }.is_ok()
                    && unsafe { GetClientRect(hwnd, &mut client) }.is_ok()
                {
                    let border = [
                        outer.right - outer.left - client.right,
                        outer.bottom - outer.top - client.bottom,
                    ];
                    let scale = f64::from(unsafe { GetDpiForWindow(hwnd) }) / 96.0;
                    let [width, height] =
                        super::size((position.cx - border[0]).max(1) as u32, scale);
                    position.cx = width as i32 + border[0];
                    position.cy = height as i32 + border[1];
                }
            }
        }
        if msg == WM_SIZING && data == 0 && (1..=8).contains(&wp.0) && lp.0 != 0 {
            let mut outer = RECT::default();
            let mut client = RECT::default();
            // WM_SIZING provides a live writable RECT for the duration of this callback.
            if unsafe { GetWindowRect(hwnd, &mut outer) }.is_ok()
                && unsafe { GetClientRect(hwnd, &mut client) }.is_ok()
            {
                let rect = unsafe { &mut *(lp.0 as *mut RECT) };
                let border = [
                    outer.right - outer.left - client.right,
                    outer.bottom - outer.top - client.bottom,
                ];
                let scale = f64::from(unsafe { GetDpiForWindow(hwnd) }) / 96.0;
                let [left, top, right, bottom] = sizing(
                    [rect.left, rect.top, rect.right, rect.bottom],
                    border,
                    wp.0,
                    scale,
                );
                *rect = RECT {
                    left,
                    top,
                    right,
                    bottom,
                };
                return LRESULT(1);
            }
        }
        if msg == WM_NCDESTROY {
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(procedure), id);
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_edges_preserve_client_ratio_and_the_opposite_anchor() {
        let original = [100, 200, 1017, 843];
        for edge in 1..=8 {
            let rect = sizing(original, [16, 39], edge, 1.0);
            let width = rect[2] - rect[0] - 16;
            let height = rect[3] - rect[1] - 39;
            assert_eq!(width * 9, height * 16);
            assert!(width >= 640 && height >= 360);
            if matches!(edge, 1 | 4 | 7) {
                assert_eq!(rect[2], original[2]);
            } else {
                assert_eq!(rect[0], original[0]);
            }
            if matches!(edge, 3..=5) {
                assert_eq!(rect[3], original[3]);
            } else {
                assert_eq!(rect[1], original[1]);
            }
        }
    }

    #[test]
    fn minimum_dpi_rounding_and_restoration_are_stable() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for width in [0, 1, 639, 1280, 1901, 2048] {
                let result = size(width, scale);
                assert_eq!(result[0] * 9, result[1] * 16);
                assert!(f64::from(result[0]) >= 640.0 * scale);
                assert_eq!(size(result[0], scale), result);
            }
        }
        assert_eq!(size(INITIAL[0], 1.0), INITIAL);
    }
}
