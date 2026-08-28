

#![windows_subsystem = "windows"]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY,
    FW_BOLD, FW_NORMAL, HBRUSH,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{InitCommonControlsEx, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowTextLengthW,
    GetWindowTextW, MessageBoxW, PostQuitMessage, RegisterClassW, SendMessageW, SetWindowTextW,
    ShowWindow, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, ES_AUTOHSCROLL, HMENU, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MSG, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND,
    WM_CREATE, WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CHILD, WS_MAXIMIZEBOX,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE,
};

const IDC_PATH_EDIT: usize = 101;
const IDC_BROWSE_BTN: usize = 102;
const IDC_LAUNCH_BTN: usize = 103;
const IDC_STATUS_LABEL: usize = 104;

const CONFIG_FILE_NAME: &str = "goley_launcher_config.json";
const DEFAULT_CLIENT_PATH: &str = r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin";

const DEFAULT_STATUS_WARNING: &str =
    "Uyarı: Eğer sunucu açık değilse Login ekranında çökme beklenmektedir.";

const EMBEDDED_PATCHES_TOML: &str = r#"# Data schema for optional, measured static patches.
schema_version = 1

[[patch]]
rva = 0x009374DB
original_bytes = "81 FE 55 07 00 00"
patched_bytes = "81 FE 7C 01 00 00"
note = "Measured 2026-08-16: accept updater-unavailable GameGuard status 380 at the exact initialization gate; login screen reached"
build_sha256 = "C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA"

[[patch]]
rva = 0x0093BB67
original_bytes = "E8 94 84 BA FF"
patched_bytes = "B8 55 07 00 00"
note = "Measured 2026-08-16: first failing periodic GameGuard poll returned 0x262 at RVA 0x0093BB73; report success only to the Error99 consumer while leaving the shared status function unchanged"
build_sha256 = "C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA"
"#;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct LauncherConfig {
    client_path: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            client_path: DEFAULT_CLIENT_PATH.to_string(),
        }
    }
}

static CURRENT_CONFIG: Mutex<Option<LauncherConfig>> = Mutex::new(None);
static UI_ELEMENTS: Mutex<Option<UiHandles>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct UiHandles {
    edit_path: usize,
    status_label: usize,
}

impl UiHandles {
    fn edit_hwnd(self) -> HWND {
        HWND(self.edit_path as *mut _)
    }
    fn status_hwnd(self) -> HWND {
        HWND(self.status_label as *mut _)
    }
}

fn get_config_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.join(CONFIG_FILE_NAME);
        }
    }
    PathBuf::from(CONFIG_FILE_NAME)
}

fn load_config() -> LauncherConfig {
    let path = get_config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<LauncherConfig>(&data) {
            return cfg;
        }
    }
    LauncherConfig::default()
}

fn save_config(cfg: &LauncherConfig) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path, json);
    }
}

fn open_file_dialog(owner: HWND) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog_result = CoCreateInstance::<_, IFileOpenDialog>(
            &FileOpenDialog,
            None,
            CLSCTX_INPROC_SERVER,
        );

        let Ok(dialog) = dialog_result else {
            CoUninitialize();
            return None;
        };

        let _ = dialog.SetOptions(FOS_FILEMUSTEXIST);
        let _ = dialog.SetTitle(w!("Goley Oyun Dosyasını Seçin (BinaryTr.bin veya BinaryTr.exe)"));

        if dialog.Show(Some(owner)).is_ok() {
            if let Ok(item) = dialog.GetResult() {
                if let Ok(pwstr) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                    let path_str = pwstr.to_string().ok();
                    CoTaskMemFree(Some(pwstr.as_ptr().cast()));
                    CoUninitialize();
                    return path_str;
                }
            }
        }
        CoUninitialize();
    }
    None
}

fn resolve_patches_file(exe_dir: &Path) -> PathBuf {
    let candidates = [
        exe_dir.join("patches.toml"),
        exe_dir.join(r"..\..\..\crates\goley-shim\patches\patches.toml"),
        exe_dir.join(r"crates\goley-shim\patches\patches.toml"),
        exe_dir.join(r"..\crates\goley-shim\patches\patches.toml"),
        exe_dir.join(r"..\..\crates\goley-shim\patches\patches.toml"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

let auto_created = exe_dir.join("patches.toml");
    let _ = fs::write(&auto_created, EMBEDDED_PATCHES_TOML);
    auto_created
}

fn launch_goley(client_path: &str, hwnd: HWND) -> anyhow::Result<()> {
    let client_file = Path::new(client_path);
    if !client_file.exists() {
        anyhow::bail!("Seçilen oyun dosyası bulunamadı:\n{}", client_path);
    }

let final_client_path = if client_file
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("bin"))
        .unwrap_or(false)
    {
        let exe_target = client_file.with_extension("exe");
        fs::copy(client_file, &exe_target).map_err(|e| {
            anyhow::anyhow!(
                "BinaryTR.bin dosyası .exe olarak kopyalanamadı ({:?} -> {:?}):\n{}",
                client_file,
                exe_target,
                e
            )
        })?;
        exe_target
    } else {
        client_file.to_path_buf()
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let search_roots = [
        exe_dir.clone(),
        exe_dir.join(r"APP\CALENTON\release"),
        exe_dir.join(r"..\APP\CALENTON\release"),
        exe_dir.join(r"..\..\APP\CALENTON\release"),
        exe_dir.join(r"..\..\..\APP\CALENTON\release"),
        exe_dir.join(r"APP\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\APP\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\..\APP\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\..\..\APP\i686-pc-windows-msvc\release"),
        exe_dir.join(r"target\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\target\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\..\target\i686-pc-windows-msvc\release"),
        exe_dir.join(r"..\..\..\target\i686-pc-windows-msvc\release"),
    ];

    let mut boot_exe: Option<PathBuf> = None;
    let mut shim_dll: Option<PathBuf> = None;

    for root in &search_roots {
        let boot = root.join("goley-boot.exe");
        let shim = root.join("goley_shim.dll");
        if boot.exists() && boot_exe.is_none() {
            boot_exe = Some(boot);
        }
        if shim.exists() && shim_dll.is_none() {
            shim_dll = Some(shim);
        }
    }

    let boot_exe = boot_exe.ok_or_else(|| {
        anyhow::anyhow!("goley-boot.exe bulunamadı. Lütfen önce projeyi derleyin (build.bat).")
    })?;

    let shim_dll = shim_dll.ok_or_else(|| {
        anyhow::anyhow!("goley_shim.dll bulunamadı. Lütfen önce projeyi derleyin.")
    })?;

    let patches_toml = resolve_patches_file(&exe_dir);

    let mut cmd = Command::new(&boot_exe);
    cmd.env("NMRunEnv_VER", "0");
    cmd.env("NMRunEnv_ENUM", "NMRunEnv_DATA_1");
    cmd.env(
        "NMRunEnv_DATA_1",
        "fc2329727b4856f29186165dcc4dfe822fa1c2959e48f21aa738aedfb42c1c2ccaaafb29cb7efa641dbe726dd07a810241ac4b6b5edb2c305d473a0f5f8b6386206d0f4b985160b36d662872e5537ea86f685615565cf9bd3739018742b38d23",
    );

    cmd.args([
        "run",
        "--client",
        final_client_path.to_str().unwrap_or(client_path),
        "--region",
        "TRAuth",
        "--runparam-key",
        "NMRP20260816LOCALKEY0001",
        "--oep-rva",
        "0x009374DB",
        "--late-inject-ms",
        "3000",
        "--shim",
        shim_dll.to_str().unwrap_or("goley_shim.dll"),
        "--patches",
        patches_toml.to_str().unwrap_or("patches.toml"),
        "--entry",
        "127.0.0.1:2270",
        "--timeout",
        "150",
        "-vv",
    ]);

    cmd.spawn()?;

    unsafe {
        MessageBoxW(
            Some(hwnd),
            w!("Goley başarıyla başlatıldı!\nOyun penceresi açılıyor, lütfen bekleyin...\n\nUyarı: Eğer sunucu açık değilse Login ekranında çökme beklenmektedir."),
            w!("Goley Başlatıcı"),
            MB_OK | MB_ICONINFORMATION,
        );
    }

    Ok(())
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let font = CreateFontW(
                16, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
                FONT_CHARSET(1),
                FONT_OUTPUT_PRECISION(0),
                FONT_CLIP_PRECISION(0),
                FONT_QUALITY(0),
                0,
                w!("Segoe UI"),
            );
            let font_bold = CreateFontW(
                16, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
                FONT_CHARSET(1),
                FONT_OUTPUT_PRECISION(0),
                FONT_CLIP_PRECISION(0),
                FONT_QUALITY(0),
                0,
                w!("Segoe UI"),
            );

let lbl = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Goley Oyun Dosyası (BinaryTr.bin / BinaryTr.exe):"),
                WS_CHILD | WS_VISIBLE,
                20, 15, 480, 20,
                Some(hwnd), None, None, None,
            ).unwrap();
            SendMessageW(lbl, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));

let cfg = load_config();
            let initial_path: Vec<u16> = cfg.client_path.encode_utf16().chain(Some(0)).collect();
            *CURRENT_CONFIG.lock().unwrap() = Some(cfg);

            let edit = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                PCWSTR(initial_path.as_ptr()),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32),
                20, 40, 380, 26,
                Some(hwnd),
                Some(HMENU(IDC_PATH_EDIT as *mut _)),
                None, None,
            ).unwrap();
            SendMessageW(edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));

let btn_browse = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                w!("Gözat..."),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_PUSHBUTTON as u32),
                410, 39, 90, 28,
                Some(hwnd),
                Some(HMENU(IDC_BROWSE_BTN as *mut _)),
                None, None,
            ).unwrap();
            SendMessageW(btn_browse, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));

let btn_launch = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                w!("GOLEY'İ BAŞLAT"),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32),
                20, 85, 480, 42,
                Some(hwnd),
                Some(HMENU(IDC_LAUNCH_BTN as *mut _)),
                None, None,
            ).unwrap();
            SendMessageW(btn_launch, WM_SETFONT, Some(WPARAM(font_bold.0 as usize)), Some(LPARAM(1)));

let initial_warn: Vec<u16> = DEFAULT_STATUS_WARNING.encode_utf16().chain(Some(0)).collect();
            let status = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(initial_warn.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20, 140, 480, 45,
                Some(hwnd),
                Some(HMENU(IDC_STATUS_LABEL as *mut _)),
                None, None,
            ).unwrap();
            SendMessageW(status, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));

            *UI_ELEMENTS.lock().unwrap() = Some(UiHandles {
                edit_path: edit.0 as usize,
                status_label: status.0 as usize,
            });

            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as usize;
            match id {
                IDC_BROWSE_BTN => {
                    if let Some(selected) = open_file_dialog(hwnd) {
                        let wide: Vec<u16> = selected.encode_utf16().chain(Some(0)).collect();
                        if let Some(ui) = UI_ELEMENTS.lock().unwrap().as_ref() {
                            let _ = SetWindowTextW(ui.edit_hwnd(), PCWSTR(wide.as_ptr()));
                        }
                        let cfg = LauncherConfig { client_path: selected };
                        save_config(&cfg);
                        *CURRENT_CONFIG.lock().unwrap() = Some(cfg);
                    }
                }
                IDC_LAUNCH_BTN => {
                    if let Some(ui) = UI_ELEMENTS.lock().unwrap().as_ref() {
                        let len = GetWindowTextLengthW(ui.edit_hwnd());
                        let mut buf = vec![0u16; (len + 1) as usize];
                        GetWindowTextW(ui.edit_hwnd(), &mut buf);
                        let path_str = String::from_utf16_lossy(&buf[..len as usize]);

                        let cfg = LauncherConfig { client_path: path_str.clone() };
                        save_config(&cfg);
                        *CURRENT_CONFIG.lock().unwrap() = Some(cfg);

                        let _ = SetWindowTextW(ui.status_hwnd(), w!("Durum: Oyun başlatılıyor, .exe hazırlanıyor..."));

                        match launch_goley(&path_str, hwnd) {
                            Ok(_) => {
                                let status_msg = format!("Durum: Goley başlatıldı!\n{}", DEFAULT_STATUS_WARNING);
                                let wide: Vec<u16> = status_msg.encode_utf16().chain(Some(0)).collect();
                                let _ = SetWindowTextW(ui.status_hwnd(), PCWSTR(wide.as_ptr()));
                            }
                            Err(e) => {
                                let _ = SetWindowTextW(ui.status_hwnd(), w!("Durum: Hata oluştu!"));
                                let err_msg = format!("Başlatma Hatası:\n{}", e);
                                let err_wide: Vec<u16> = err_msg.encode_utf16().chain(Some(0)).collect();
                                MessageBoxW(
                                    Some(hwnd),
                                    PCWSTR(err_wide.as_ptr()),
                                    w!("Hata"),
                                    MB_OK | MB_ICONERROR,
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() -> anyhow::Result<()> {
    unsafe {
        let mut icce = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&mut icce);

        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("GoleyLauncherWindowClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class_name,
            hbrBackground: HBRUSH(15 as *mut _),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Goley Client Launcher"),
            WINDOW_STYLE(
                (WS_OVERLAPPEDWINDOW.0 & !WS_THICKFRAME.0 & !WS_MAXIMIZEBOX.0)
                    | WS_VISIBLE.0,
            ),
            100, 100, 540, 235,
            None, None, Some(HINSTANCE(instance.0)), None,
        ).unwrap();

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = DispatchMessageW(&msg);
        }
    }

    Ok(())
}
