use dbpx::{decode, rgb_bytes};
use std::env;
use std::error::Error;
use std::path::Path;

#[cfg(not(windows))]
fn main() -> Result<(), Box<dyn Error>> {
    eprintln!("dbpx-view is currently supported on Windows only");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn Error>> {
    windows_view::run()
}

#[cfg(windows)]
mod windows_view {
    use super::*;
    use std::ffi::c_void;
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use std::sync::OnceLock;

    type Hinstance = *mut c_void;
    type Hwnd = *mut c_void;
    type Hdc = *mut c_void;
    type Hicon = *mut c_void;
    type Hcursor = *mut c_void;
    type Hbrush = *mut c_void;
    type Hmenu = *mut c_void;
    type Wparam = usize;
    type Lparam = isize;
    type Lresult = isize;

    const CS_VREDRAW: u32 = 0x0001;
    const CS_HREDRAW: u32 = 0x0002;
    const CW_USEDEFAULT: i32 = 0x80000000u32 as i32;
    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
    const SW_SHOW: i32 = 5;
    const WM_DESTROY: u32 = 0x0002;
    const WM_PAINT: u32 = 0x000F;
    const WM_KEYDOWN: u32 = 0x0100;
    const VK_ESCAPE: usize = 0x1B;
    const DIB_RGB_COLORS: u32 = 0;
    const SRCCOPY: u32 = 0x00CC0020;

    static VIEW: OnceLock<ViewState> = OnceLock::new();

    struct ViewState {
        width: i32,
        height: i32,
        pixels: Vec<u32>,
    }

    #[repr(C)]
    struct WndClassW {
        style: u32,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hinstance,
        h_icon: Hicon,
        h_cursor: Hcursor,
        hbr_background: Hbrush,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        w_param: Wparam,
        l_param: Lparam,
        time: u32,
        pt: Point,
    }

    #[repr(C)]
    struct PaintStruct {
        hdc: Hdc,
        f_erase: i32,
        rc_paint: Rect,
        f_restore: i32,
        f_inc_update: i32,
        rgb_reserved: [u8; 32],
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct RgbQuad {
        rgb_blue: u8,
        rgb_green: u8,
        rgb_red: u8,
        rgb_reserved: u8,
    }

    #[repr(C)]
    struct BitmapInfo {
        bmi_header: BitmapInfoHeader,
        bmi_colors: [RgbQuad; 1],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(wnd_class: *const WndClassW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hmenu,
            instance: Hinstance,
            param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn ShowWindow(hwnd: Hwnd, cmd_show: i32) -> i32;
        fn UpdateWindow(hwnd: Hwnd) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> Lresult;
        fn PostQuitMessage(exit_code: i32);
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn BeginPaint(hwnd: Hwnd, paint: *mut PaintStruct) -> Hdc;
        fn EndPaint(hwnd: Hwnd, paint: *const PaintStruct) -> i32;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn StretchDIBits(
            hdc: Hdc,
            x_dest: i32,
            y_dest: i32,
            dest_width: i32,
            dest_height: i32,
            x_src: i32,
            y_src: i32,
            src_width: i32,
            src_height: i32,
            bits: *const c_void,
            bitmap_info: *const BitmapInfo,
            usage: u32,
            rop: u32,
        ) -> i32;
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let input = env::args().nth(1).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "usage: dbpx-view <input.dbpx>")
        })?;
        let data = std::fs::read(&input)?;
        let image = decode(&data)?;
        let width = i32::try_from(image.width)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "image width too large"))?;
        let height = i32::try_from(image.height)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "image height too large"))?;
        let rgb = rgb_bytes(&image);
        let mut pixels = Vec::with_capacity((image.width as usize) * (image.height as usize));
        for px in rgb.chunks_exact(3) {
            pixels.push(((px[0] as u32) << 16) | ((px[1] as u32) << 8) | (px[2] as u32));
        }
        if VIEW.set(ViewState { width, height, pixels }).is_err() {
            return Err(io::Error::new(io::ErrorKind::Other, "viewer state already initialized").into());
        }

        let title = Path::new(&input)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("DBPX Viewer");
        show_window(title, width, height)
    }

    fn show_window(title: &str, image_width: i32, image_height: i32) -> Result<(), Box<dyn Error>> {
        let class_name = wide("DBPX_VIEW_WINDOW");
        let title = wide(title);
        unsafe {
            let instance = GetModuleHandleW(null());
            let wnd_class = WndClassW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfn_wnd_proc: Some(wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: null_mut(),
                h_cursor: null_mut(),
                hbr_background: null_mut(),
                lpsz_menu_name: null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            if RegisterClassW(&wnd_class) == 0 {
                return Err(io::Error::last_os_error().into());
            }

            let window_width = image_width.saturating_mul(8).clamp(320, 960);
            let window_height = image_height.saturating_mul(8).clamp(240, 720);
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                window_width,
                window_height,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                return Err(io::Error::last_os_error().into());
            }
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);

            let mut msg: Msg = zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: Hwnd,
        msg: u32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        match msg {
            WM_PAINT => {
                let mut paint: PaintStruct = zeroed();
                let hdc = BeginPaint(hwnd, &mut paint);
                if let Some(view) = VIEW.get() {
                    let mut rect: Rect = zeroed();
                    GetClientRect(hwnd, &mut rect);
                    let dest_width = (rect.right - rect.left).max(1);
                    let dest_height = (rect.bottom - rect.top).max(1);
                    let bmi = BitmapInfo {
                        bmi_header: BitmapInfoHeader {
                            bi_size: size_of::<BitmapInfoHeader>() as u32,
                            bi_width: view.width,
                            bi_height: -view.height,
                            bi_planes: 1,
                            bi_bit_count: 32,
                            bi_compression: 0,
                            bi_size_image: 0,
                            bi_x_pels_per_meter: 2835,
                            bi_y_pels_per_meter: 2835,
                            bi_clr_used: 0,
                            bi_clr_important: 0,
                        },
                        bmi_colors: [RgbQuad {
                            rgb_blue: 0,
                            rgb_green: 0,
                            rgb_red: 0,
                            rgb_reserved: 0,
                        }],
                    };
                    StretchDIBits(
                        hdc,
                        0,
                        0,
                        dest_width,
                        dest_height,
                        0,
                        0,
                        view.width,
                        view.height,
                        view.pixels.as_ptr().cast(),
                        &bmi,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }
                EndPaint(hwnd, &paint);
                0
            }
            WM_KEYDOWN if wparam == VK_ESCAPE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}
