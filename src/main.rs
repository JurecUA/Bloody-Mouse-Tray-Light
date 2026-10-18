#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, SYSTEMTIME, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    System::Registry::{RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ},
    System::{SystemInformation::GetLocalTime, Time::SystemTimeToFileTime},
    Graphics::{Dwm::DwmSetWindowAttribute, Gdi::{
        CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
        CreatePen, CreateSolidBrush, DrawTextW, FillRect, FrameRect, GetTextExtentPoint32W, InvalidateRect, Polyline, RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, HBRUSH, HDC, UpdateWindow, BITMAPINFO, PAINTSTRUCT, PS_SOLID,
        BI_RGB, COLOR_BTNFACE, DIB_RGB_COLORS, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK,
        FW_BOLD, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY,
        DEFAULT_PITCH,
    }},
    UI::{
        Controls::{DRAWITEMSTRUCT, ODT_COMBOBOX, ODS_SELECTED},
        Shell::{NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW, ShellExecuteW},
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage,
            RegisterClassW, SetForegroundWindow, SetTimer, KillTimer, TrackPopupMenu, TranslateMessage, SW_SHOWNORMAL,
            BringWindowToTop, ShowWindow, SendMessageW, DestroyWindow, GetClientRect,
            GetWindow, PostMessageW,
            WNDCLASSW, CS_HREDRAW, CS_VREDRAW, MF_SEPARATOR, MF_STRING, MSG,
            TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY, WM_DRAWITEM, WM_LBUTTONUP,
            WM_RBUTTONUP, WM_TIMER, WM_CLOSE, WM_CREATE, WM_ERASEBKGND, WM_PAINT, WM_CTLCOLORSTATIC, WM_CTLCOLORBTN,
            WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORDLG, WS_OVERLAPPED,
            WS_CHILD, WS_VISIBLE, WS_TABSTOP, WS_VSCROLL, CBS_DROPDOWNLIST, CBS_OWNERDRAWFIXED, CBS_HASSTRINGS,
            CB_SETITEMHEIGHT, CB_GETLBTEXTLEN, CB_GETLBTEXT,
            BS_OWNERDRAW,
            CB_ADDSTRING, CB_SETCURSEL, CB_GETCURSEL, SW_SHOW, CreateIconIndirect, DestroyIcon, SetWindowTextW,
            WS_CAPTION, WS_SYSMENU, WS_MINIMIZEBOX,
            GW_CHILD, GW_HWNDNEXT,
            ICONINFO,
        },
    },
};

const WM_TRAY: u32 = WM_APP + 1;
const ID_SETTINGS: usize = 1001;
const ID_EXIT: usize = 1099;
const ID_ABOUT: usize = 1098;
const ID_RESET_TIMER: usize = 1097;
const ID_UI_LANGUAGE: usize = 2001;
const ID_UI_POLL: usize = 2002;
const ID_UI_DARK: usize = 2010;
const ID_UI_BATTERY: usize = 2011;
const ID_UI_OUTLINE: usize = 2012;
const ID_UI_COLORED: usize = 2013;
const ID_UI_DEBUG: usize = 2014;
const ID_UI_AUTOSTART: usize = 2018;
const ID_UI_DEFAULTS: usize = 2020;
const ID_UI_CANCEL: usize = 2021;
const ID_UI_OK: usize = 2022;
const ID_ABOUT_LINK: usize = 2090;
const ID_ABOUT_CLOSE: usize = 2091;
const TIMER_ID: usize = 1;
const CHARGE_ANIMATION_TIMER_ID: usize = 2;
const HOVER_ICON_TIMER_ID: usize = 3;
const WM_SETTINGS_REBUILD: u32 = WM_APP + 2;
const WM_STATUS_RESULT: u32 = WM_APP + 3;
const CBN_SELCHANGE_CODE: usize = 1;
const BN_CLICKED_CODE: usize = 0;
const SS_LEFTNOWORDWRAP: u32 = 0x000c;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    poll_rate: u64,
    text_outline: bool,
    colored_icon: bool,
    battery_icon: bool,
    dark_theme: bool,
    debug: bool,
    #[serde(skip)]
    autostart: bool,
    language: String,
}

impl Default for Settings {
    fn default() -> Self {
        // Defaults intentionally match the user's portable setup.
        Self { poll_rate: 60, text_outline: false,
            colored_icon: true, battery_icon: false,
            dark_theme: true, debug: false, autostart:false, language: "ru".into() }
    }
}

#[derive(Default)]
struct Status { percent: Option<u8>, charging: bool, full: bool, animation_percent: Option<u8>, rate: Option<u16>, dpi: Option<u16>, present: bool }
#[derive(Clone,Default)]
struct MouseMemory { last_percent:Option<u8>, full_charge:Option<String> }

fn load_mouse_memory()->MouseMemory {
    let path=config_path();
    let Ok(bytes)=fs::read(path) else { return MouseMemory::default() };
    let Ok(root)=serde_json::from_slice::<serde_json::Value>(&bytes) else { return MouseMemory::default() };
    let Some(mouse)=root.get("mice").and_then(|v|v.get("Bloody SG5 Pro")) else { return MouseMemory::default() };
    MouseMemory { last_percent:mouse.get("last_percent").and_then(|v|v.as_u64()).filter(|v|*v<=100).map(|v|v as u8), full_charge:mouse.get("full_charge").and_then(|v|v.as_str()).map(str::to_owned) }
}
fn save_mouse_memory(memory:&MouseMemory) {
    let path=config_path();
    let mut root:serde_json::Value=fs::read(&path).ok().and_then(|v|serde_json::from_slice(&v).ok()).unwrap_or_else(||serde_json::json!({}));
    root["mice"]["Bloody SG5 Pro"]["last_percent"]=memory.last_percent.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    root["mice"]["Bloody SG5 Pro"]["full_charge"]=memory.full_charge.clone().map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
    if let Ok(bytes)=serde_json::to_vec_pretty(&root) { let tmp=path.with_extension("json.tmp"); if fs::write(&tmp,bytes).is_ok() { let _=fs::rename(tmp,path); } }
}
fn filetime_ticks(ft:FILETIME)->u64 { ((ft.dwHighDateTime as u64)<<32)|ft.dwLowDateTime as u64 }
fn timestamp_ticks(value:&str)->Option<u64> {
    let mut halves=value.split('T'); let date=halves.next()?; let time=halves.next()?;
    let mut d=date.split('-'); let year=d.next()?.parse().ok()?; let month=d.next()?.parse().ok()?; let day=d.next()?.parse().ok()?;
    let mut t=time.split(':'); let hour=t.next()?.parse().ok()?; let minute=t.next()?.parse().ok()?; let second=t.next()?.parse().ok()?;
    let st=SYSTEMTIME{wYear:year,wMonth:month,wDay:day,wHour:hour,wMinute:minute,wSecond:second,..Default::default()};
    let mut ft=FILETIME::default(); if unsafe {SystemTimeToFileTime(&st,&mut ft)}==0 {None} else {Some(filetime_ticks(ft))}
}
fn now_local_string()->String {
    let mut st=SYSTEMTIME::default(); unsafe {GetLocalTime(&mut st)};
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",st.wYear,st.wMonth,st.wDay,st.wHour,st.wMinute,st.wSecond)
}
fn update_mouse_memory(memory:&mut MouseMemory,status:&Status,charging_below_full:&mut bool)->bool {
    let Some(percent)=status.percent else{return false};
    if status.charging || status.full {
        *charging_below_full=percent<100;
    } else if percent<100 {
        *charging_below_full=false;
    } else if *charging_below_full {
        return false;
    }
    let charge_is_new=percent==100 && memory.last_percent.map(|p|p<100).unwrap_or(memory.full_charge.is_none());
    let mut changed=memory.last_percent!=Some(percent);
    if charge_is_new { memory.full_charge=Some(now_local_string()); changed=true; }
    memory.last_percent=Some(percent);
    changed
}

fn wide(s: &str) -> Vec<u16> { OsStr::new(s).encode_wide().chain(Some(0)).collect() }
fn app_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."))
}
fn config_path() -> PathBuf { app_dir().join("mouse_tray.json") }
fn load_settings() -> Settings {
    let mut settings:Settings=fs::read(config_path()).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("settings").cloned()).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    settings.autostart=autostart_enabled();
    settings
}
fn save_settings(s: &Settings) {
    let path = config_path();
    let mut root: serde_json::Value = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| serde_json::json!({}));
    root["settings"] = serde_json::to_value(s).unwrap_or_default();
    if let Ok(bytes) = serde_json::to_vec_pretty(&root) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, bytes).is_ok() { let _ = fs::rename(tmp, path); }
    }
}
fn write_debug_log(settings:&Settings,message:&str) {
    if !settings.debug { return; }
    use std::io::Write;
    let dir=app_dir().join("logs");
    if fs::create_dir_all(&dir).is_err() { return; }
    let path=dir.join("app.log");
    if let Ok(mut file)=fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|v|v.as_secs()).unwrap_or_default();
        let _=writeln!(file,"{secs} {message}");
    }
}
fn autostart_enabled()->bool {
    let key_path=wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name=wide("BloodyMouseTrayLight");
    let mut key:HKEY=ptr::null_mut();
    if unsafe {RegOpenKeyExW(HKEY_CURRENT_USER,key_path.as_ptr(),0,KEY_QUERY_VALUE,&mut key)}!=0 {return false;}
    let mut kind=0u32; let mut bytes=0u32;
    let result=unsafe {RegQueryValueExW(key,value_name.as_ptr(),ptr::null(),&mut kind,ptr::null_mut(),&mut bytes)};
    unsafe {RegCloseKey(key);}
    result==0 && kind==REG_SZ
}
fn set_autostart(enabled:bool)->Result<(),std::io::Error> {
    let key_path=wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name=wide("BloodyMouseTrayLight");
    let mut key:HKEY=ptr::null_mut();
    let result=unsafe {RegOpenKeyExW(HKEY_CURRENT_USER,key_path.as_ptr(),0,KEY_SET_VALUE,&mut key)};
    if result!=0 {return Err(std::io::Error::from_raw_os_error(result as i32));}
    let result=if enabled {
        let command=wide(&format!("\"{}\"",std::env::current_exe()?.display()));
        unsafe {RegSetValueExW(key,value_name.as_ptr(),0,REG_SZ,command.as_ptr() as *const u8,(command.len()*2) as u32)}
    } else {
        let code=unsafe {RegDeleteValueW(key,value_name.as_ptr())};
        if code==2 {0}else{code}
    };
    unsafe {RegCloseKey(key);}
    if result==0 {Ok(())}else{Err(std::io::Error::from_raw_os_error(result as i32))}
}

fn read_status() -> Status {
    const VID: u16 = 0x1956;
    const PIDS: [u16; 2] = [0x3144, 0x3069];
    const PING: [u8; 9] = [0,0x80,0,0,0,0,0,0,0x7f];
    const QUERY: [u8; 9] = [0,0x80,0xff,0,0,0,0,0,0x80];
    let Ok(api) = HidApi::new() else { return Status::default() };
    for pid in PIDS {
        let mut status_info = None;
        let mut command_info = None;
        for d in api.device_list().filter(|d| d.vendor_id() == VID && d.product_id() == pid) {
            match (d.usage_page(), d.usage()) {
                (0xff01, 1) if status_info.is_none() => status_info = Some(d.path().to_owned()),
                (0xff02, 1) if command_info.is_none() => command_info = Some(d.path().to_owned()),
                _ => {}
            }
        }
        let (Some(sp), Some(cp)) = (status_info, command_info) else { continue };
        let Ok(status_dev) = api.open_path(&sp) else { continue };
        let Ok(cmd) = api.open_path(&cp) else { continue };
        let _ = cmd.send_feature_report(&PING);
        thread::sleep(Duration::from_millis(20));
        if cmd.send_feature_report(&QUERY).is_err() { continue; }
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let mut result = Status { present: true, ..Default::default() };
        while std::time::Instant::now() < deadline {
            let mut buf = [0u8; 8];
            let ms = (deadline - std::time::Instant::now()).as_millis() as i32;
            if let Ok(n) = status_dev.read_timeout(&mut buf, ms.max(1)) {
                if n < 8 || buf[0] != 6 { continue; }
                match buf[1] {
                    0x56 if buf[2] <= 100 => { result.percent = Some(buf[2]); result.charging = buf[3] != 0 && buf[2] < 100; result.full = buf[3] != 0 && buf[2] == 100; }
                    0x81 => result.rate = [1000,500,250,125,8000,4000,2000].get(buf[2] as usize).copied(),
                    0x87 => result.dpi = [400,800,1000,1200,1600,2000].get(buf[7] as usize).copied(),
                    _ => {}
                }
                if result.percent.is_some() && result.rate.is_some() && result.dpi.is_some() { break; }
            } else { break; }
        }
        if result.percent.is_some() { return result; }
    }
    Status::default()
}

fn tr(lang: &str, en: &'static str, ru: &'static str, uk: &'static str) -> &'static str {
    match lang { "ru" => ru, "uk" => uk, _ => en }
}
fn tooltip(s: &Settings, st: &Status, memory:&MouseMemory) -> String {
    let title="Bloody SG5 Pro";
    if !st.present { return tr(&s.language,"No Mouse Detected","Мышь не обнаружена","Мишу не виявлено").to_string(); }
    let mut tip=title.to_string();
    if s.battery_icon {
        if let Some(percent)=st.percent { tip.push_str(&format!("\n{percent}%")); }
    }
    if let Some(full)=memory.full_charge.as_deref() {
        if let (Some(now),Some(then))=(timestamp_ticks(&now_local_string()),timestamp_ticks(full)) {
            let elapsed=now.saturating_sub(then)/10_000_000;
            let days=elapsed/86_400; let hours=(elapsed/3_600)%24; let minutes=(elapsed/60)%60; let seconds=elapsed%60;
            tip.push_str(&format!("\n{days}{}{:02}:{:02}:{:02}",tr(&s.language," days, "," дн., "," дн., "),hours,minutes,seconds));
        }
    }
    let mut sensor=Vec::new();
    if let Some(rate)=st.rate { sensor.push(format!("{rate} Hz")); }
    if let Some(dpi)=st.dpi { sensor.push(format!("DPI {dpi}")); }
    if !sensor.is_empty() { tip.push_str(&format!("\n{}",sensor.join(" · "))); }
    tip
}

fn indicator_color(settings: &Settings, percent: Option<u8>) -> [u8;3] {
    if !settings.colored_icon { return [255,255,255]; }
    let Some(p)=percent else { return [255,255,255]; };
    if p>=50 { return [0,255,0]; }
    if p>=20 {
        let t=(50-p) as f32/30.0;
        return [(255.0*t).round() as u8,255,0];
    }
    if p>=5 {
        let t=(20-p) as f32/15.0;
        return [255,(255.0*(1.0-t)).round() as u8,0];
    }
    [255,0,0]
}

fn report_rate_text(rate:Option<u16>)->String {
    match rate {
        Some(v) if v>=1000 && v%1000==0 => format!("{}K",v/1000),
        Some(v) => v.to_string(),
        None => "—".into(),
    }
}

unsafe fn render_tray_icon(settings: &Settings, status: &Status, show_rate:bool) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    const EDGE:i32=32;
    let mut info=BITMAPINFO::default();
    info.bmiHeader.biSize=std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth=EDGE;
    info.bmiHeader.biHeight=-EDGE;
    info.bmiHeader.biPlanes=1;
    info.bmiHeader.biBitCount=32;
    info.bmiHeader.biCompression=BI_RGB;
    let mut pixels: *mut core::ffi::c_void=ptr::null_mut();
    let bitmap=unsafe { CreateDIBSection(ptr::null_mut(),&info,DIB_RGB_COLORS,&mut pixels,ptr::null_mut(),0) };
    if bitmap.is_null() || pixels.is_null() { return ptr::null_mut(); }
    unsafe { ptr::write_bytes(pixels,0,(EDGE*EDGE*4) as usize); }
    let color=if show_rate {[255,255,255]}else{indicator_color(settings,status.percent)};
    if !show_rate && (settings.battery_icon || status.full || status.charging) && status.present && status.percent.is_some() {
        let level=if status.charging {status.animation_percent.or(status.percent).unwrap_or(0)} else {status.percent.unwrap_or(0)} as i32;
        let bgr=[color[2].max(1),color[1].max(1),color[0].max(1),255];
        let raw=pixels as *mut u8;
        let set_pixel=|x:i32,y:i32| { if (0..EDGE).contains(&x)&&(0..EDGE).contains(&y) { unsafe { ptr::copy_nonoverlapping(bgr.as_ptr(),raw.add(((y*EDGE+x)*4) as usize),4); } } };
        for y in 9..23 { for x in 5..26 { if x<7||x>=24||y<11||y>=21 { set_pixel(x,y); } } }
        for y in 13..19 { for x in 26..30 { set_pixel(x,y); } }
        let fill=(17*level/100).clamp(0,17);
        for y in 12..20 { for x in 7..(7+fill) { set_pixel(x,y); } }
    } else {
        let text=if show_rate { report_rate_text(status.rate) } else if !status.present { "—".to_string() } else { status.percent.map(|p|p.to_string()).unwrap_or_else(||"?".into()) };
        let face=wide("Calibri");
        let dc=unsafe { CreateCompatibleDC(ptr::null_mut()) };
        if !dc.is_null() {
            unsafe { SelectObject(dc,bitmap as _); SetBkMode(dc,1); }
            let mut font=ptr::null_mut();
            let text_wide=wide(&text);
            let text_len=text.encode_utf16().count() as i32;
            // Render Calibri about 30% larger while keeping its measured glyphs inside the tray icon.
            for px in (8..=64).rev() {
                let candidate=unsafe { CreateFontW(-px*130/100,0,0,0,FW_BOLD as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,face.as_ptr()) };
                if candidate.is_null() { continue; }
                unsafe { SelectObject(dc,candidate as _); }
                let mut extent=SIZE::default();
                let fits=unsafe { GetTextExtentPoint32W(dc,text_wide.as_ptr(),text_len,&mut extent) }!=0 && extent.cx<=32 && extent.cy<=32;
                if fits { font=candidate; break; }
                unsafe { DeleteObject(candidate as _); }
            }
            if !font.is_null() {
            let draw_text=|rgb:[u8;3], dx:i32, dy:i32| {
                let rgbref=(rgb[0] as u32)|((rgb[1] as u32)<<8)|((rgb[2] as u32)<<16);
                let mut rect=RECT{left:dx,top:dy,right:EDGE+dx,bottom:EDGE+dy};
                unsafe { SetTextColor(dc,rgbref); DrawTextW(dc,text_wide.as_ptr(),text_len,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX); }
            };
            if settings.text_outline {
                let outline=[0,0,0];
                for (dx,dy) in [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] { draw_text(outline,dx,dy); }
            }
            draw_text(color,0,0);
            unsafe { DeleteDC(dc); DeleteObject(font as _); }
            let buf=unsafe { std::slice::from_raw_parts_mut(pixels as *mut u8,(EDGE*EDGE*4) as usize) };
            for p in buf.chunks_exact_mut(4) { p[3]=if p[0]|p[1]|p[2] == 0 {0}else{255}; }
            } else { unsafe { DeleteDC(dc); } }
        }
    }
    let mask=unsafe { CreateBitmap(EDGE,EDGE,1,1,ptr::null()) };
    if mask.is_null() { unsafe { DeleteObject(bitmap as _); } return ptr::null_mut(); }
    let icon_info=ICONINFO{fIcon:1,xHotspot:0,yHotspot:0,hbmMask:mask,hbmColor:bitmap};
    let icon=unsafe { CreateIconIndirect(&icon_info) };
    unsafe { DeleteObject(bitmap as _); DeleteObject(mask as _); }
    icon
}

struct App { nid: NOTIFYICONDATAW, settings: Settings, status: Status, poll_in_progress: bool, was_full: bool, animation_active: bool, animation_index: usize, hover_rate_icon:bool, charging_below_full:bool, memory:MouseMemory, settings_hwnd: HWND, about_hwnd: HWND, settings_before: Option<Settings>, current_icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON, brush_dark:HBRUSH, brush_light:HBRUSH, brush_card_dark:HBRUSH, brush_card_light:HBRUSH }
static mut APP: *mut App = ptr::null_mut();

const LANGUAGE_CLASS: &str = "BloodyMouseTraySettingsWindow";
const ABOUT_CLASS: &str = "BloodyMouseTrayAboutWindow";

unsafe fn create_child(parent: HWND, class: &str, text: &str, style: u32, ex_style: u32, id: usize, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let c=wide(class); let t=wide(text);
    unsafe { CreateWindowExW(ex_style,c.as_ptr(),t.as_ptr(),style,x,y,w,h,parent,id as *mut _,GetModuleHandleW(ptr::null()),ptr::null()) }
}

unsafe fn fill_combo(hwnd: HWND, id: usize, entries: &[String], selected: usize) {
    let combo=unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd,id as i32) };
    for entry in entries { let w=wide(entry); unsafe { SendMessageW(combo,CB_ADDSTRING,0,w.as_ptr() as isize); } }
    unsafe { SendMessageW(combo,CB_SETCURSEL,selected,0); }
    unsafe { SendMessageW(combo,CB_SETITEMHEIGHT,usize::MAX,32); SendMessageW(combo,CB_SETITEMHEIGHT,0,32); }
}

unsafe fn draw_combo_item(item:&DRAWITEMSTRUCT, dark:bool) {
    if item.CtlType!=ODT_COMBOBOX { return; }
    let selected=item.itemState&ODS_SELECTED!=0;
    let (background,foreground,highlight,border,arrow)=if dark {
        (0x00333333,0x00ffffff,0x00454545,0x00525252,0x00d0d0d0)
    } else {
        (0x00ffffff,0x00141414,0x00e5f1fb,0x00bfc8d2,0x00555555)
    };
    let fill=unsafe { CreateSolidBrush(if selected {highlight}else{background}) };
    unsafe { FillRect(item.hDC,&item.rcItem,fill); DeleteObject(fill as _); }
    let mut rect=item.rcItem;
    let is_control=item.itemID==u32::MAX;
    if is_control {
        let face=unsafe { CreateSolidBrush(background) };
        let outline=unsafe { CreatePen(PS_SOLID,1,border) };
        let old_brush=unsafe { SelectObject(item.hDC,face as _) };
        let old_pen=unsafe { SelectObject(item.hDC,outline as _) };
        unsafe { RoundRect(item.hDC,rect.left,rect.top,rect.right,rect.bottom,10,10); SelectObject(item.hDC,old_pen); SelectObject(item.hDC,old_brush); DeleteObject(outline as _); DeleteObject(face as _); }
        rect.right-=26;
    }
    let text_index=if is_control {unsafe { SendMessageW(item.hwndItem,CB_GETCURSEL,0,0) as usize}}else{item.itemID as usize};
    if text_index!=usize::MAX {
        let length=unsafe { SendMessageW(item.hwndItem,CB_GETLBTEXTLEN,text_index,0) as usize };
        if length<256 {
            let mut text=vec![0u16;length+1];
            unsafe { SendMessageW(item.hwndItem,CB_GETLBTEXT,text_index,text.as_mut_ptr() as isize); }
            rect.left+=10;
            rect.right-=2;
            unsafe { SetBkMode(item.hDC,1); SetTextColor(item.hDC,foreground); DrawTextW(item.hDC,text.as_ptr(),-1,&mut rect,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX); }
        }
    }
    if is_control {
        let pen=unsafe { CreatePen(PS_SOLID,2,arrow) };
        let old=unsafe { SelectObject(item.hDC,pen as _) };
        let x=item.rcItem.right-15;
        let y=(item.rcItem.top+item.rcItem.bottom)/2-2;
        let points=[POINT{x:x-5,y},POINT{x,y:y+5},POINT{x:x+5,y}];
        unsafe { Polyline(item.hDC,points.as_ptr(),points.len() as i32); SelectObject(item.hDC,old); DeleteObject(pen as _); }
    }
}

unsafe fn draw_action_button(item:&DRAWITEMSTRUCT,settings:&Settings) {
    let dark=settings.dark_theme;
    let is_ok=item.CtlID as usize==ID_UI_OK;
    let (background,foreground,border)=if is_ok {
        if dark {(0x0065c9fa,0x00151515,0x0065c9fa)}else{(0x000078d4,0x00ffffff,0x000078d4)}
    } else if dark {(0x00373737,0x00f0f0f0,0x00484848)} else {(0x00e8e8e8,0x00141414,0x00d0d0d0)};
    let fill=unsafe {CreateSolidBrush(background)};
    let pen=unsafe {CreatePen(PS_SOLID,1,border)};
    let old_fill=unsafe {SelectObject(item.hDC,fill as _)};
    let old_pen=unsafe {SelectObject(item.hDC,pen as _)};
    let r=&item.rcItem;
    unsafe {RoundRect(item.hDC,r.left+1,r.top+1,r.right-1,r.bottom-1,10,10);SelectObject(item.hDC,old_pen);SelectObject(item.hDC,old_fill);DeleteObject(pen as _);DeleteObject(fill as _);}
    let label=match item.CtlID as usize {
        ID_UI_DEFAULTS=>tr(&settings.language,"Defaults","По умолчанию","За замовчуванням"),
        ID_UI_CANCEL=>tr(&settings.language,"Cancel","Отмена","Скасувати"),
        ID_UI_OK=>tr(&settings.language,"OK","ОК","Гаразд"),
        _=>return,
    };
    let text=wide(label);let mut rect=*r;
    unsafe {SetBkMode(item.hDC,1);SetTextColor(item.hDC,foreground);DrawTextW(item.hDC,text.as_ptr(),-1,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);}
}

unsafe fn draw_about_button(item:&DRAWITEMSTRUCT,settings:&Settings) {
    let dark=settings.dark_theme;
    let (background,foreground,border)=if dark {(0x00373737,0x00f0f0f0,0x00484848)} else {(0x00e8e8e8,0x00141414,0x00d0d0d0)};
    let fill=unsafe {CreateSolidBrush(background)};
    let pen=unsafe {CreatePen(PS_SOLID,1,border)};
    let old_fill=unsafe {SelectObject(item.hDC,fill as _)};
    let old_pen=unsafe {SelectObject(item.hDC,pen as _)};
    let r=&item.rcItem;
    unsafe {RoundRect(item.hDC,r.left+1,r.top+1,r.right-1,r.bottom-1,10,10);SelectObject(item.hDC,old_pen);SelectObject(item.hDC,old_fill);DeleteObject(pen as _);DeleteObject(fill as _);}
    let label=if item.CtlID as usize==ID_ABOUT_LINK {
        tr(&settings.language,"Open project page","Открыть страницу проекта","Відкрити сторінку проєкту")
    } else { tr(&settings.language,"Close","Закрыть","Закрити") };
    let text=wide(label);let mut rect=*r;
    unsafe {SetBkMode(item.hDC,1);SetTextColor(item.hDC,foreground);DrawTextW(item.hDC,text.as_ptr(),-1,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);}
}

fn checkbox_is_checked(settings:&Settings,id:u32)->bool {
    match id as usize {
        ID_UI_DARK=>settings.dark_theme,
        ID_UI_BATTERY=>settings.battery_icon,
        ID_UI_OUTLINE=>settings.text_outline,
        ID_UI_COLORED=>settings.colored_icon,
        ID_UI_AUTOSTART=>settings.autostart,
        ID_UI_DEBUG=>settings.debug,
        _=>false,
    }
}

unsafe fn draw_checkbox(item:&DRAWITEMSTRUCT,settings:&Settings) {
    let id=item.CtlID;
    let label=match id as usize {
        ID_UI_DARK=>tr(&settings.language,"Dark theme","Тёмная тема","Темна тема"),
        ID_UI_BATTERY=>tr(&settings.language,"Battery icon","Значок батареи","Піктограма батареї"),
        ID_UI_OUTLINE=>tr(&settings.language,"Text outline","Обводка текста","Контур тексту"),
        ID_UI_COLORED=>tr(&settings.language,"Colored icon","Цветной значок","Кольоровий значок"),
        ID_UI_AUTOSTART=>tr(&settings.language,"Start with Windows","Запускать вместе с Windows","Запускати разом із Windows"),
        ID_UI_DEBUG=>tr(&settings.language,"Debug logging","Журнал отладки","Журнал налагодження"),
        _=>return,
    };
    let dark=settings.dark_theme;
    let (card,fg,border,accent)=if dark {(0x002d2d2d,0x00ffffff,0x00777777,0x0065c9fa)}else{(0x00ffffff,0x00141414,0x00777777,0x000078d4)};
    let fill=unsafe { CreateSolidBrush(card) };
    unsafe { FillRect(item.hDC,&item.rcItem,fill); DeleteObject(fill as _); }
    let box_rect=RECT{left:item.rcItem.left,top:item.rcItem.top+5,right:item.rcItem.left+17,bottom:item.rcItem.top+22};
    let box_fill=unsafe { CreateSolidBrush(if checkbox_is_checked(settings,id) {accent}else{card}) };
    unsafe { FillRect(item.hDC,&box_rect,box_fill); DeleteObject(box_fill as _); }
    let box_border=unsafe { CreateSolidBrush(if checkbox_is_checked(settings,id) {accent}else{border}) };
    unsafe { FrameRect(item.hDC,&box_rect,box_border); DeleteObject(box_border as _); }
    if checkbox_is_checked(settings,id) {
        let pen=unsafe { CreatePen(PS_SOLID,2,0x00ffffff) };
        let old=unsafe { SelectObject(item.hDC,pen as _) };
        let points=[POINT{x:box_rect.left+3,y:box_rect.top+9},POINT{x:box_rect.left+7,y:box_rect.top+13},POINT{x:box_rect.left+14,y:box_rect.top+4}];
        unsafe { Polyline(item.hDC,points.as_ptr(),points.len() as i32); SelectObject(item.hDC,old); DeleteObject(pen as _); }
    }
    let text=wide(label);
    let mut text_rect=RECT{left:item.rcItem.left+26,top:item.rcItem.top,right:item.rcItem.right,bottom:item.rcItem.bottom};
    unsafe { SetBkMode(item.hDC,1); SetTextColor(item.hDC,fg); DrawTextW(item.hDC,text.as_ptr(),-1,&mut text_rect,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX); }
}

fn selected_combo(hwnd: HWND, id: usize) -> usize {
    let combo=unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd,id as i32) };
    unsafe { SendMessageW(combo,CB_GETCURSEL,0,0) as usize }
}

fn poll_interval_ms(settings:&Settings,status:&Status)->u32 {
    let seconds=if status.present {settings.poll_rate.clamp(1,3600)}else{1};
    (seconds*1000).min(u32::MAX as u64) as u32
}

unsafe fn selected_combo_text(hwnd:HWND,id:usize)->String {
    let combo=unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd,id as i32) };
    let index=unsafe { SendMessageW(combo,CB_GETCURSEL,0,0) };
    if index<0 { return String::new(); }
    let length=unsafe { SendMessageW(combo,CB_GETLBTEXTLEN,index as usize,0) };
    if !(0..256).contains(&length) { return String::new(); }
    let mut text=vec![0u16;length as usize+1];
    unsafe { SendMessageW(combo,CB_GETLBTEXT,index as usize,text.as_mut_ptr() as isize); }
    String::from_utf16_lossy(&text[..length as usize])
}

unsafe fn create_settings_controls(hwnd: HWND, s: &Settings) {
    unsafe {
        create_child(hwnd,"STATIC",tr(&s.language,"Language","Язык","Мова"),WS_CHILD|WS_VISIBLE|SS_LEFTNOWORDWRAP,0,0,30,377,116,28);
        create_child(hwnd,"COMBOBOX","",WS_CHILD|WS_VISIBLE|WS_TABSTOP|WS_VSCROLL|(CBS_DROPDOWNLIST|CBS_OWNERDRAWFIXED|CBS_HASSTRINGS) as u32,0,ID_UI_LANGUAGE,152,373,222,180);
        let languages=["English","Русский","Українська"].iter().map(|x|x.to_string()).collect::<Vec<_>>();
        fill_combo(hwnd,ID_UI_LANGUAGE,&languages,match s.language.as_str(){"ru"=>1,"uk"=>2,_=>0});
        create_child(hwnd,"STATIC",tr(&s.language,"Poll interval","Интервал опроса","Інтервал опитування"),WS_CHILD|WS_VISIBLE|SS_LEFTNOWORDWRAP,0,0,30,421,116,28);
        create_child(hwnd,"COMBOBOX","",WS_CHILD|WS_VISIBLE|WS_TABSTOP|WS_VSCROLL|(CBS_DROPDOWNLIST|CBS_OWNERDRAWFIXED|CBS_HASSTRINGS) as u32,0,ID_UI_POLL,152,417,222,180);
        let mut poll_values=vec![1u64,2,5,10,15,30,60,120,300,600,1800,3600];
        if !poll_values.contains(&s.poll_rate) { poll_values.push(s.poll_rate.clamp(1,3600)); poll_values.sort_unstable(); }
        let poll_selected=poll_values.iter().position(|v| *v==s.poll_rate.clamp(1,3600)).unwrap_or(6);
        let poll_labels=poll_values.iter().map(u64::to_string).collect::<Vec<_>>();
        fill_combo(hwnd,ID_UI_POLL,&poll_labels,poll_selected);
        let checks=[
            (ID_UI_DARK,tr(&s.language,"Dark theme","Тёмная тема","Темна тема"),s.dark_theme,182),
            (ID_UI_BATTERY,tr(&s.language,"Battery icon","Значок батареи","Піктограма батареї"),s.battery_icon,214),
            (ID_UI_OUTLINE,tr(&s.language,"Text outline","Обводка текста","Контур тексту"),s.text_outline,246),
            (ID_UI_COLORED,tr(&s.language,"Colored icon","Цветной значок","Кольоровий значок"),s.colored_icon,278),
            (ID_UI_AUTOSTART,tr(&s.language,"Start with Windows","Запускать вместе с Windows","Запускати разом із Windows"),s.autostart,465),
            (ID_UI_DEBUG,tr(&s.language,"Debug logging","Журнал отладки","Журнал налагодження"),s.debug,501),
        ];
        for (id,label,checked,y) in checks.iter() {
            let control=create_child(hwnd,"BUTTON",label,WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,0,*id,30,*y,344,28);
            let _=*checked;
            let _=control;
        }
        create_child(hwnd,"BUTTON",tr(&s.language,"Defaults","По умолчанию","За замовчуванням"),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,0,ID_UI_DEFAULTS,30,578,140,36);
        create_child(hwnd,"BUTTON",tr(&s.language,"Cancel","Отмена","Скасувати"),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,0,ID_UI_CANCEL,214,578,76,36);
        create_child(hwnd,"BUTTON",tr(&s.language,"OK","ОК","Гаразд"),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,0,ID_UI_OK,300,578,74,36);
    }
}

unsafe fn rebuild_settings_controls(hwnd:HWND,s:&Settings) {
    let mut child=unsafe { GetWindow(hwnd,GW_CHILD) };
    while !child.is_null() {
        let next=unsafe { GetWindow(child,GW_HWNDNEXT) };
        unsafe { DestroyWindow(child); }
        child=next;
    }
    unsafe { create_settings_controls(hwnd,s); }
}

unsafe fn read_settings_controls(hwnd: HWND, s: &mut Settings) {
    let lang=selected_combo(hwnd,ID_UI_LANGUAGE);
    s.language=match lang {1=>"ru",2=>"uk",_=>"en"}.into();
    s.poll_rate=unsafe {selected_combo_text(hwnd,ID_UI_POLL)}.parse::<u64>().unwrap_or(60).clamp(1,3600);
}

unsafe fn apply_dialog_theme(hwnd:HWND,app:&App) {
    let dark=if app.settings.dark_theme {1i32}else{0i32};
    unsafe {
        DwmSetWindowAttribute(hwnd,20,&dark as *const _ as *const core::ffi::c_void,std::mem::size_of::<i32>() as u32);
        InvalidateRect(hwnd,ptr::null(),1);
    }
}

unsafe fn paint_settings(hwnd:HWND,dc:HDC,app:&App) {
    let (bg,card,border,fg,muted,accent)=if app.settings.dark_theme {
        (0x00242424,0x002d2d2d,0x003b3b3b,0x00ffffff,0x00b8b8b8,0x0065c9fa)
    } else {
        (0x00f2f2f2,0x00ffffff,0x00dddddd,0x00141414,0x00666666,0x000078d4)
    };
    let mut client=RECT::default();
    unsafe { GetClientRect(hwnd,&mut client); }
    let background=unsafe { CreateSolidBrush(bg) };
    unsafe { FillRect(dc,&client,background); DeleteObject(background as _); }
    let panel=unsafe { CreateSolidBrush(card) };
    let pen=unsafe { CreatePen(PS_SOLID,1,border) };
    let old_panel=unsafe { SelectObject(dc,panel as _) };
    let old_pen=unsafe { SelectObject(dc,pen as _) };
    for (top,bottom) in [(14,126),(138,322),(336,560)] {
        unsafe { RoundRect(dc,14,top,390,bottom,18,18); }
    }
    unsafe { SelectObject(dc,old_panel); SelectObject(dc,old_pen); DeleteObject(panel as _); DeleteObject(pen as _); SetBkMode(dc,1); }

    let font_name=wide("Segoe UI");
    let heading_font=unsafe { CreateFontW(-20,0,0,0,FW_BOLD as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,font_name.as_ptr()) };
    let body_font=unsafe { CreateFontW(-15,0,0,0,400,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,font_name.as_ptr()) };
    let bold_font=unsafe { CreateFontW(-25,0,0,0,FW_BOLD as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,font_name.as_ptr()) };
    let draw=|text:&str,x:i32,y:i32,w:i32,h:i32,color:u32| {
        let text=wide(text);
        let mut rect=RECT{left:x,top:y,right:x+w,bottom:y+h};
        unsafe { SetTextColor(dc,color); DrawTextW(dc,text.as_ptr(),-1,&mut rect,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX); }
    };
    let old=unsafe { SelectObject(dc,heading_font as _) };
    draw(tr(&app.settings.language,"Bloody SG5 Pro","Bloody SG5 Pro","Bloody SG5 Pro"),30,22,340,30,fg);
    draw(tr(&app.settings.language,"Appearance","Внешний вид","Вигляд"),30,146,340,30,fg);
    draw(tr(&app.settings.language,"System","Система","Система"),30,344,340,30,fg);
    let battery=if app.status.present { app.status.percent.map(|p|format!("{p}%")).unwrap_or_else(||"?".into()) }
        else { tr(&app.settings.language,"No mouse detected","Мышь не обнаружена","Мишу не виявлено").to_string() };
    unsafe { SelectObject(dc,body_font as _); }
    draw(tr(&app.settings.language,"Battery","Батарея","Батарея"),30,57,75,31,muted);
    unsafe { SelectObject(dc,bold_font as _); }
    draw(&battery,110,50,120,42,if app.status.full {0x0000cc66}else{fg});
    unsafe { SelectObject(dc,body_font as _); }
    let detail=if !app.status.present { tr(&app.settings.language,"Connect the mouse to see its status","Подключите мышь, чтобы увидеть состояние","Підключіть мишу, щоб побачити стан").to_string() }
        else { format!("{}: {} Hz   ·   DPI: {}",tr(&app.settings.language,"Report rate","Частота опроса","Частота опитування"),app.status.rate.map(|v|v.to_string()).unwrap_or_else(||"—".into()),app.status.dpi.map(|v|v.to_string()).unwrap_or_else(||"—".into())) };
    draw(&detail,30,80,344,24,muted);
    let track=unsafe { CreateSolidBrush(if app.settings.dark_theme {0x00454545}else{0x00e0e0e0}) };
    let track_rect=RECT{left:30,top:108,right:374,bottom:115};
    unsafe { FillRect(dc,&track_rect,track); DeleteObject(track as _); }
    if let Some(percent)=app.status.percent.filter(|_|app.status.present) {
        let fill_color=if app.status.full {0x0000cc66}else{accent};
        let fill=unsafe { CreateSolidBrush(fill_color) };
        let width=(344*percent as i32/100).max(3);
        let fill_rect=RECT{left:30,top:108,right:30+width,bottom:115};
        unsafe { FillRect(dc,&fill_rect,fill); DeleteObject(fill as _); }
    }
    unsafe { SelectObject(dc,old); DeleteObject(heading_font as _); DeleteObject(body_font as _); DeleteObject(bold_font as _); }
}

unsafe fn paint_about(hwnd:HWND,dc:HDC,app:&App) {
    let (bg,card,border,fg,muted,accent)=if app.settings.dark_theme {
        (0x00242424,0x002d2d2d,0x003b3b3b,0x00ffffff,0x00b8b8b8,0x0065c9fa)
    } else {
        (0x00f2f2f2,0x00ffffff,0x00dddddd,0x00141414,0x00666666,0x000078d4)
    };
    let mut client=RECT::default();
    unsafe { GetClientRect(hwnd,&mut client); }
    let background=unsafe { CreateSolidBrush(bg) };
    unsafe { FillRect(dc,&client,background); DeleteObject(background as _); }
    let panel=unsafe { CreateSolidBrush(card) };
    let pen=unsafe { CreatePen(PS_SOLID,1,border) };
    let old_panel=unsafe { SelectObject(dc,panel as _) };
    let old_pen=unsafe { SelectObject(dc,pen as _) };
    unsafe { RoundRect(dc,18,18,622,306,18,18); SelectObject(dc,old_panel); SelectObject(dc,old_pen); DeleteObject(panel as _); DeleteObject(pen as _); SetBkMode(dc,1); }
    let family=wide("Segoe UI");
    let heading=unsafe { CreateFontW(-24,0,0,0,FW_BOLD as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,family.as_ptr()) };
    let subhead=unsafe { CreateFontW(-17,0,0,0,FW_BOLD as i32,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,family.as_ptr()) };
    let body=unsafe { CreateFontW(-15,0,0,0,400,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,family.as_ptr()) };
    let draw=|text:&str,x:i32,y:i32,w:i32,h:i32,color:u32,flags:u32| {
        let text=wide(text); let mut rect=RECT{left:x,top:y,right:x+w,bottom:y+h};
        unsafe { SetTextColor(dc,color); DrawTextW(dc,text.as_ptr(),-1,&mut rect,flags); }
    };
    let old=unsafe { SelectObject(dc,heading as _) };
    draw("Bloody Mouse Tray Light",40,36,500,36,fg,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
    unsafe { SelectObject(dc,body as _); }
    let intro=tr(&app.settings.language,
        "Battery and report-rate indicator for the Bloody SG5 Pro wireless mouse.",
        "Индикатор заряда и частоты опроса беспроводной мыши Bloody SG5 Pro.",
        "Індикатор заряду й частоти опитування бездротової миші Bloody SG5 Pro.");
    draw(intro,40,82,550,52,fg,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);
    unsafe { SelectObject(dc,subhead as _); }
    draw(tr(&app.settings.language,"Supported device","Поддерживаемое устройство","Підтримуваний пристрій"),40,154,550,28,fg,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
    unsafe { SelectObject(dc,body as _); }
    draw("Bloody SG5 Pro  ·  wireless dongle 1956:3069  ·  cable 1956:3144",40,186,550,32,muted,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
    draw(tr(&app.settings.language,"Portable settings are stored next to the program. Works without Bloody Central.","Настройки хранятся рядом с программой. Работает без Bloody Central.","Налаштування зберігаються поруч із програмою. Працює без Bloody Central."),40,232,550,48,muted,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);
    unsafe { SetTextColor(dc,accent); draw("github.com/JurecUA/Bloody-Mouse-Tray",40,280,560,24,accent,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX); SelectObject(dc,old); DeleteObject(heading as _); DeleteObject(subhead as _); DeleteObject(body as _); }
}

unsafe extern "system" fn about_wndproc(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM)->LRESULT {
    if msg==WM_CREATE {
        unsafe {
            let app_ptr=APP;
            if let Some(app)=app_ptr.as_mut() {
                app.about_hwnd=hwnd;
                let dark=if app.settings.dark_theme {1i32}else{0i32};
                DwmSetWindowAttribute(hwnd,20,&dark as *const _ as *const core::ffi::c_void,std::mem::size_of::<i32>() as u32);
                let link=wide(tr(&app.settings.language,"Open project page","Открыть страницу проекта","Відкрити сторінку проєкту"));
                let close=wide(tr(&app.settings.language,"Close","Закрыть","Закрити"));
                let button=wide("BUTTON");
                CreateWindowExW(0,button.as_ptr(),link.as_ptr(),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,40,328,220,36,hwnd,ID_ABOUT_LINK as *mut _,GetModuleHandleW(ptr::null()),ptr::null());
                CreateWindowExW(0,button.as_ptr(),close.as_ptr(),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_OWNERDRAW as u32,510,328,90,36,hwnd,ID_ABOUT_CLOSE as *mut _,GetModuleHandleW(ptr::null()),ptr::null());
            }
        }
        return 0;
    }
    if msg==WM_ERASEBKGND { return 1; }
    if [WM_CTLCOLORBTN,WM_CTLCOLORSTATIC].contains(&msg) {
        let app_ptr=unsafe {APP};
        if let Some(app)=unsafe {app_ptr.as_ref()} {
            let dark=app.settings.dark_theme;
            unsafe { SetBkMode(wp as HDC,2); SetBkColor(wp as HDC,if dark {0x002d2d2d}else{0x00ffffff}); SetTextColor(wp as HDC,if dark {0x00ffffff}else{0x00000000}); }
            return (if dark {app.brush_card_dark}else{app.brush_card_light}) as LRESULT;
        }
    }
    if msg==WM_DRAWITEM && lp!=0 {
        let item=unsafe {&*(lp as *const DRAWITEMSTRUCT)};
        if [ID_ABOUT_LINK as u32,ID_ABOUT_CLOSE as u32].contains(&item.CtlID) {
            if let Some(app)=unsafe {APP.as_ref()} { unsafe {draw_about_button(item,&app.settings);} return 1; }
        }
    }
    if msg==WM_PAINT {
        let mut ps=PAINTSTRUCT::default(); let dc=unsafe { windows_sys::Win32::Graphics::Gdi::BeginPaint(hwnd,&mut ps) };
        let app_ptr=unsafe { APP }; if let Some(app)=unsafe { app_ptr.as_ref() } { unsafe { paint_about(hwnd,dc,app); } }
        unsafe { windows_sys::Win32::Graphics::Gdi::EndPaint(hwnd,&ps); } return 0;
    }
    if msg==WM_COMMAND {
        match (wp&0xffff) as usize {
            ID_ABOUT_LINK=>{let verb=wide("open"); let url=wide("https://github.com/JurecUA/Bloody-Mouse-Tray"); unsafe { ShellExecuteW(hwnd,verb.as_ptr(),url.as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL); } return 0;},
            ID_ABOUT_CLOSE=>{unsafe { DestroyWindow(hwnd); } return 0;},
            _=>{}
        }
    }
    if msg==WM_CLOSE { unsafe { DestroyWindow(hwnd); } return 0; }
    if msg==WM_DESTROY { let app_ptr=unsafe {APP}; if let Some(app)=unsafe {app_ptr.as_mut()} { app.about_hwnd=ptr::null_mut(); } return 0; }
    unsafe { DefWindowProcW(hwnd,msg,wp,lp) }
}

unsafe extern "system" fn settings_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg==WM_CREATE {
        let app_ptr=unsafe { APP };
        let app=unsafe { app_ptr.as_ref() };
        if let Some(app)=app { unsafe { create_settings_controls(hwnd,&app.settings); apply_dialog_theme(hwnd,app); } }
        return 0;
    }
    if msg==WM_ERASEBKGND {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe {app_ptr.as_ref()} {
            let brush=if app.settings.dark_theme {app.brush_dark}else{app.brush_light};
            let mut rect=RECT::default(); unsafe {GetClientRect(hwnd,&mut rect); FillRect(wp as HDC,&rect,brush);}
            return 1;
        }
    }
    if msg==WM_PAINT {
        let mut ps=PAINTSTRUCT::default();
        let dc=unsafe { windows_sys::Win32::Graphics::Gdi::BeginPaint(hwnd,&mut ps) };
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_ref() } { unsafe { paint_settings(hwnd,dc,app); } }
        unsafe { windows_sys::Win32::Graphics::Gdi::EndPaint(hwnd,&ps); }
        return 0;
    }
    if msg==WM_DRAWITEM && lp!=0 {
        let item=unsafe { &*(lp as *const DRAWITEMSTRUCT) };
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_ref() } {
            if [ID_UI_LANGUAGE as u32,ID_UI_POLL as u32].contains(&item.CtlID) {
                unsafe { draw_combo_item(item,app.settings.dark_theme); }
                return 1;
            }
            if [ID_UI_DARK as u32,ID_UI_BATTERY as u32,ID_UI_OUTLINE as u32,ID_UI_COLORED as u32,ID_UI_AUTOSTART as u32,ID_UI_DEBUG as u32].contains(&item.CtlID) {
                unsafe { draw_checkbox(item,&app.settings); }
                return 1;
            }
            if [ID_UI_DEFAULTS as u32,ID_UI_CANCEL as u32,ID_UI_OK as u32].contains(&item.CtlID) {
                unsafe { draw_action_button(item,&app.settings); }
                return 1;
            }
        }
    }
    if [WM_CTLCOLORSTATIC,WM_CTLCOLORBTN,WM_CTLCOLOREDIT,WM_CTLCOLORLISTBOX,WM_CTLCOLORDLG].contains(&msg) {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe {app_ptr.as_ref()} {
            let dark=app.settings.dark_theme;
            let brush=if dark {app.brush_card_dark}else{app.brush_card_light};
            let back=if dark {0x002d2d2d}else{0x00ffffff};
            let fore=if dark {0x00ffffff}else{0x00000000};
            unsafe {SetBkMode(wp as HDC,2);SetBkColor(wp as HDC,back);SetTextColor(wp as HDC,fore);}
            return brush as LRESULT;
        }
    }
    if msg==WM_SETTINGS_REBUILD {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_mut() } {
            unsafe { rebuild_settings_controls(hwnd,&app.settings); apply_dialog_theme(hwnd,app); }
            let title=wide(tr(&app.settings.language,"Bloody Mouse Tray Light — Settings","Bloody Mouse Tray Light — Настройки","Bloody Mouse Tray Light — Налаштування"));
            unsafe { SetWindowTextW(hwnd,title.as_ptr()); }
        }
        return 0;
    }
    if msg==WM_COMMAND {
        let id=(wp&0xffff) as usize;
        let notification=((wp>>16)&0xffff) as usize;
        if id==ID_UI_OK && notification==BN_CLICKED_CODE { unsafe { close_settings(hwnd,true); } return 0; }
        if id==ID_UI_CANCEL && notification==BN_CLICKED_CODE { unsafe { close_settings(hwnd,false); } return 0; }
        if [ID_UI_LANGUAGE,ID_UI_POLL].contains(&id) && notification!=CBN_SELCHANGE_CODE { return 0; }
        if [ID_UI_DARK,ID_UI_BATTERY,ID_UI_OUTLINE,ID_UI_COLORED,ID_UI_DEBUG,ID_UI_AUTOSTART,ID_UI_DEFAULTS].contains(&id) && notification!=BN_CLICKED_CODE { return 0; }
        let app_ptr=unsafe { APP };
        let app=unsafe { app_ptr.as_mut() };
        if let Some(app)=app {
            if id==ID_UI_DEFAULTS {
                app.settings=Settings::default();
                unsafe { PostMessageW(hwnd,WM_SETTINGS_REBUILD,0,0); update_icon(app); }
                return 0;
            }
            if id==ID_UI_LANGUAGE {
                unsafe { read_settings_controls(hwnd,&mut app.settings); PostMessageW(hwnd,WM_SETTINGS_REBUILD,0,0); update_icon(app); }
                return 0;
            }
        if [ID_UI_DARK,ID_UI_BATTERY,ID_UI_OUTLINE,ID_UI_COLORED,ID_UI_DEBUG,ID_UI_AUTOSTART].contains(&id) {
            unsafe { read_settings_controls(hwnd,&mut app.settings); }
            match id {
                ID_UI_DARK=>app.settings.dark_theme=!app.settings.dark_theme,
                ID_UI_BATTERY=>app.settings.battery_icon=!app.settings.battery_icon,
                ID_UI_OUTLINE=>app.settings.text_outline=!app.settings.text_outline,
                ID_UI_COLORED=>app.settings.colored_icon=!app.settings.colored_icon,
                ID_UI_DEBUG=>app.settings.debug=!app.settings.debug,
                ID_UI_AUTOSTART=>app.settings.autostart=!app.settings.autostart,
                _=>{}
            }
            unsafe { update_icon(app); InvalidateRect(hwnd,ptr::null(),1); if id==ID_UI_DARK {apply_dialog_theme(hwnd,app);} }
        }
        if id==ID_UI_POLL {
            unsafe { read_settings_controls(hwnd,&mut app.settings); update_icon(app); SetTimer(app.nid.hWnd,TIMER_ID,poll_interval_ms(&app.settings,&app.status),None); }
            }
        }
        return 0;
    }
    if msg==WM_CLOSE {
        unsafe { close_settings(hwnd,false); }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd,msg,wp,lp) }
}

unsafe fn close_settings(hwnd: HWND, save: bool) {
    let mut error:Option<String>=None;
    {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_mut() } {
            if save {
                unsafe { read_settings_controls(hwnd,&mut app.settings); }
                if let Err(e)=set_autostart(app.settings.autostart) { error=Some(e.to_string()); }
                if error.is_none() { save_settings(&app.settings); app.settings_before=None; }
            } else if let Some(original)=app.settings_before.take() { app.settings=original; }
            if error.is_none() {
                app.settings_hwnd=ptr::null_mut();
                unsafe { update_icon(app); SetTimer(app.nid.hWnd,TIMER_ID,poll_interval_ms(&app.settings,&app.status),None); }
            }
        }
    }
    if let Some(error)=error {
        let message=wide(&format!("Could not change the Windows startup setting:\n{error}"));
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(hwnd,message.as_ptr(),wide("Bloody Mouse Tray Light").as_ptr(),0x10); }
        return;
    }
    unsafe { DestroyWindow(hwnd); }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_TRAY && (lp as u32 == WM_RBUTTONUP || lp as u32 == WM_LBUTTONUP) {
        let app_ptr = unsafe { APP };
        let p = unsafe { app_ptr.as_mut() };
        if let Some(app) = p {
            let menu = unsafe { CreatePopupMenu() };
            if !menu.is_null() {
                let add = |id, text: &str| { let w=wide(text); unsafe { AppendMenuW(menu, MF_STRING, id, w.as_ptr()); } };
                add(ID_SETTINGS, tr(&app.settings.language,"Settings","Настройки","Налаштування"));
                unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null()); }
                add(ID_RESET_TIMER,tr(&app.settings.language,"Reset full-charge timer","Сбросить таймер заряда","Скинути таймер заряджання"));
                add(ID_ABOUT,tr(&app.settings.language,"About...","О программе...","Про програму..."));
                add(ID_EXIT,tr(&app.settings.language,"Exit","Выход","Вихід"));
                let mut point=POINT::default(); unsafe { GetCursorPos(&mut point); SetForegroundWindow(hwnd); TrackPopupMenu(menu,TPM_LEFTALIGN|TPM_BOTTOMALIGN,point.x,point.y,0,hwnd,ptr::null()); DestroyMenu(menu); }
            }
        }
        return 0;
    }
    if msg == WM_COMMAND {
        let id=(wp & 0xffff) as usize;
        let app_ptr=unsafe { APP };
        if id==ID_SETTINGS {
            let (existing,title_text)={
                if let Some(app)=unsafe { app_ptr.as_mut() } {
                    let title=tr(&app.settings.language,"Bloody Mouse Tray Light — Settings","Bloody Mouse Tray Light — Настройки","Bloody Mouse Tray Light — Налаштування").to_string();
                    if app.settings_hwnd.is_null() { app.settings_before=Some(app.settings.clone()); (None,title) }
                    else { (Some(app.settings_hwnd),title) }
                } else { return 0; }
            };
            if let Some(existing)=existing { unsafe { ShowWindow(existing,SW_SHOW); BringWindowToTop(existing); } }
            else {
                let class=wide(LANGUAGE_CLASS);
                let title=wide(&title_text);
                let instance=unsafe { GetModuleHandleW(ptr::null()) };
                let settings_style=WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU|WS_MINIMIZEBOX|WS_VISIBLE;
                let created=unsafe { CreateWindowExW(0,class.as_ptr(),title.as_ptr(),settings_style,100,40,416,680,ptr::null_mut(),ptr::null_mut(),instance,ptr::null()) };
                if let Some(app)=unsafe { app_ptr.as_mut() } { app.settings_hwnd=created; }
                if !created.is_null() { unsafe { ShowWindow(created,SW_SHOW); UpdateWindow(created); } }
            }
            return 0;
        }
        if id==ID_ABOUT {
            if let Some(app)=unsafe { app_ptr.as_mut() } {
                if !app.about_hwnd.is_null() { unsafe { ShowWindow(app.about_hwnd,SW_SHOW); BringWindowToTop(app.about_hwnd); } }
                else {
                    let class=wide(ABOUT_CLASS);
                    let title=wide(tr(&app.settings.language,"About Bloody Mouse Tray Light","О программе Bloody Mouse Tray Light","Про Bloody Mouse Tray Light"));
                    let about_style=WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU|WS_VISIBLE;
                    let created=unsafe { CreateWindowExW(0,class.as_ptr(),title.as_ptr(),about_style,180,160,660,420,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null()) };
                    if !created.is_null() { unsafe { ShowWindow(created,SW_SHOW); UpdateWindow(created); } }
                }
            }
            return 0;
        }
        if id==ID_EXIT { unsafe { PostQuitMessage(0); } return 0; }
        let p=unsafe { app_ptr.as_mut() };
        if let Some(app)=p {
            match id {
                ID_RESET_TIMER => { app.memory.full_charge=Some(now_local_string()); save_mouse_memory(&app.memory); unsafe { update_icon(app); } },
                _=>{}
            }
        }
        return 0;
    }
    if msg == WM_TIMER && wp as usize==HOVER_ICON_TIMER_ID {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_mut() } {
            let mut identifier=NOTIFYICONIDENTIFIER::default();
            identifier.cbSize=std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32;
            identifier.hWnd=app.nid.hWnd;
            identifier.uID=app.nid.uID;
            let mut icon_rect=RECT::default();
            let mut cursor=POINT::default();
            let over_icon=unsafe { Shell_NotifyIconGetRect(&identifier,&mut icon_rect)==0 && GetCursorPos(&mut cursor)!=0
                && cursor.x>=icon_rect.left && cursor.x<icon_rect.right && cursor.y>=icon_rect.top && cursor.y<icon_rect.bottom };
            let show_rate=over_icon && app.status.present && app.status.rate.is_some();
            if show_rate!=app.hover_rate_icon { app.hover_rate_icon=show_rate; unsafe { update_icon(app); } }
        }
        return 0;
    }
    if msg == WM_TIMER && wp as usize==CHARGE_ANIMATION_TIMER_ID {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_mut() } {
            if app.status.charging {
                const CHARGE_FRAMES:[u8;3]=[0,50,100];
                app.status.animation_percent=Some(CHARGE_FRAMES[app.animation_index%CHARGE_FRAMES.len()]);
                app.animation_index=(app.animation_index+1)%CHARGE_FRAMES.len();
                unsafe { update_icon(app); }
            } else {
                app.animation_active=false;
                app.status.animation_percent=None;
                unsafe { KillTimer(hwnd,CHARGE_ANIMATION_TIMER_ID); update_icon(app); }
            }
        }
        return 0;
    }
    if msg == WM_STATUS_RESULT {
        let result=unsafe { Box::from_raw(lp as *mut Status) };
        unsafe { apply_status(hwnd,*result); }
        return 0;
    }
    if msg == WM_TIMER && wp as usize==TIMER_ID {
        let app_ptr=unsafe { APP };
        if let Some(app)=unsafe { app_ptr.as_mut() } {
            if app.poll_in_progress { return 0; }
            app.poll_in_progress=true;
        }
        let hwnd_value=hwnd as isize;
        thread::spawn(move || {
            let result=Box::into_raw(Box::new(read_status()));
            let posted=unsafe { PostMessageW(hwnd_value as HWND,WM_STATUS_RESULT,0,result as LPARAM) };
            if posted==0 { unsafe { drop(Box::from_raw(result)); } }
        });
        return 0;
    }
    if msg == WM_DESTROY { unsafe { PostQuitMessage(0); } return 0; }
    unsafe { DefWindowProcW(hwnd,msg,wp,lp) }
}

unsafe fn apply_status(hwnd:HWND,status:Status) {
    let app_ptr=unsafe { APP };
    if let Some(app)=unsafe { app_ptr.as_mut() } {
        app.poll_in_progress=false;
        app.status=status;
        if update_mouse_memory(&mut app.memory,&app.status,&mut app.charging_below_full) { save_mouse_memory(&app.memory); }
        write_debug_log(&app.settings,&format!("status: present={} percent={:?} charging={} polling={:?} dpi={:?}",app.status.present,app.status.percent,app.status.charging,app.status.rate,app.status.dpi));
        if app.status.charging && !app.animation_active {
            app.animation_index=0;
            app.animation_active=true;
            unsafe { SetTimer(hwnd,CHARGE_ANIMATION_TIMER_ID,500,None); }
        } else if !app.status.charging && app.animation_active {
            app.animation_active=false;
            app.status.animation_percent=None;
            unsafe { KillTimer(hwnd,CHARGE_ANIMATION_TIMER_ID); }
        }
        let reached_full=app.status.full && !app.was_full;
        app.was_full=app.status.full;
        if reached_full { unsafe { show_full_notification(app); } }
        unsafe { update_icon(app); }
        if !app.settings_hwnd.is_null() { unsafe { InvalidateRect(app.settings_hwnd,ptr::null(),1); } }
        // Match the original app's quick reconnect detection while the mouse is absent.
        unsafe { SetTimer(hwnd,TIMER_ID,poll_interval_ms(&app.settings,&app.status),None); }
    }
}

unsafe fn update_icon(app: &mut App) {
    let new_icon=unsafe { render_tray_icon(&app.settings,&app.status,app.hover_rate_icon) };
    if !new_icon.is_null() {
        let old_icon=app.current_icon;
        app.current_icon=new_icon;
        app.nid.hIcon=new_icon;
        unsafe { Shell_NotifyIconW(windows_sys::Win32::UI::Shell::NIM_MODIFY,&mut app.nid); }
        if !old_icon.is_null() { unsafe { DestroyIcon(old_icon); } }
    }
    app.nid.szTip.fill(0);
    let tip=wide(&tooltip(&app.settings,&app.status,&app.memory));
    let count=tip.len().min(app.nid.szTip.len());
    app.nid.szTip[..count].copy_from_slice(&tip[..count]);
    unsafe { Shell_NotifyIconW(windows_sys::Win32::UI::Shell::NIM_MODIFY,&mut app.nid); }
}

unsafe fn show_full_notification(app:&mut App) {
    let title=wide("Bloody Mouse Tray Light");
    let message=wide(tr(&app.settings.language,"The mouse battery is fully charged.","Батарея мыши полностью заряжена.","Акумулятор миші повністю заряджений."));
    app.nid.szInfo.fill(0);
    app.nid.szInfoTitle.fill(0);
    let info_len=message.len().min(app.nid.szInfo.len());
    let title_len=title.len().min(app.nid.szInfoTitle.len());
    app.nid.szInfo[..info_len].copy_from_slice(&message[..info_len]);
    app.nid.szInfoTitle[..title_len].copy_from_slice(&title[..title_len]);
    app.nid.dwInfoFlags=NIIF_INFO;
    app.nid.uFlags=NIF_INFO;
    unsafe { Shell_NotifyIconW(NIM_MODIFY,&mut app.nid); }
    app.nid.uFlags=NIF_MESSAGE|NIF_ICON|NIF_TIP;
    app.nid.szInfo.fill(0);
    app.nid.szInfoTitle.fill(0);
}

fn main() {
    let mut settings=load_settings();
    let class=wide("BloodyMouseTrayRustWindow");
    let title=wide("Bloody Mouse Tray Light");
    unsafe {
        let instance=GetModuleHandleW(ptr::null());
        let app_icon=LoadIconW(instance,1usize as *const u16);
        let wc=WNDCLASSW { style: CS_HREDRAW|CS_VREDRAW, lpfnWndProc:Some(wndproc), hInstance:instance, lpszClassName:class.as_ptr(), hIcon:app_icon, ..Default::default() };
        if RegisterClassW(&wc)==0 { return; }
        let settings_class=wide(LANGUAGE_CLASS);
        let settings_wc=WNDCLASSW { style: CS_HREDRAW|CS_VREDRAW, lpfnWndProc:Some(settings_wndproc), hInstance:instance, lpszClassName:settings_class.as_ptr(), hIcon:app_icon, hbrBackground:(COLOR_BTNFACE+1) as HBRUSH, ..Default::default() };
        RegisterClassW(&settings_wc);
        let about_class=wide(ABOUT_CLASS);
        let about_wc=WNDCLASSW { style: CS_HREDRAW|CS_VREDRAW, lpfnWndProc:Some(about_wndproc), hInstance:instance, lpszClassName:about_class.as_ptr(), hIcon:app_icon, ..Default::default() };
        RegisterClassW(&about_wc);
        let hwnd=CreateWindowExW(0,class.as_ptr(),title.as_ptr(),WS_OVERLAPPED,0,0,0,0,ptr::null_mut(),ptr::null_mut(),instance,ptr::null());
        if hwnd.is_null() { return; }
        let status=read_status();
        write_debug_log(&settings,&format!("startup: present={} percent={:?}",status.present,status.percent));
        let mut memory=load_mouse_memory();
        let mut charging_below_full=false;
        if update_mouse_memory(&mut memory,&status,&mut charging_below_full) { save_mouse_memory(&memory); }
        let icon=app_icon;
        let mut nid:NOTIFYICONDATAW=std::mem::zeroed();
        nid.cbSize=std::mem::size_of::<NOTIFYICONDATAW>() as u32; nid.hWnd=hwnd; nid.uID=1; nid.uFlags=NIF_MESSAGE|NIF_ICON|NIF_TIP; nid.uCallbackMessage=WM_TRAY; nid.hIcon=icon;
        let mut app=App { nid,settings:settings.clone(),status,poll_in_progress:false,was_full:false,animation_active:false,animation_index:0,hover_rate_icon:false,charging_below_full,memory,settings_hwnd:ptr::null_mut(),about_hwnd:ptr::null_mut(),settings_before:None,current_icon:ptr::null_mut(),brush_dark:CreateSolidBrush(0x00242424),brush_light:CreateSolidBrush(0x00f0f0f0),brush_card_dark:CreateSolidBrush(0x002d2d2d),brush_card_light:CreateSolidBrush(0x00ffffff) };
        update_icon(&mut app);
        APP=Box::into_raw(Box::new(app));
        let app_ptr=APP;
        let app=app_ptr.as_mut().unwrap();
        Shell_NotifyIconW(NIM_ADD,&mut app.nid);
        SetTimer(hwnd,HOVER_ICON_TIMER_ID,120,None);
        SetTimer(hwnd,TIMER_ID,poll_interval_ms(&settings,&app.status),None);
        let mut msg:MSG=std::mem::zeroed();
        while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0 { TranslateMessage(&msg); DispatchMessageW(&msg); }
        let app_ptr=APP;
        APP=ptr::null_mut();
        if !app_ptr.is_null() {
            let mut app=Box::from_raw(app_ptr);
            if !app.settings_hwnd.is_null() { DestroyWindow(app.settings_hwnd); }
            if !app.about_hwnd.is_null() { DestroyWindow(app.about_hwnd); }
            Shell_NotifyIconW(NIM_DELETE,&mut app.nid);
            if !app.current_icon.is_null() { DestroyIcon(app.current_icon); }
            if !app.brush_dark.is_null() { DeleteObject(app.brush_dark as _); }
            if !app.brush_light.is_null() { DeleteObject(app.brush_light as _); }
            if !app.brush_card_dark.is_null() { DeleteObject(app.brush_card_dark as _); }
            if !app.brush_card_light.is_null() { DeleteObject(app.brush_card_light as _); }
            settings=app.settings.clone();
            save_settings(&settings);
            save_mouse_memory(&app.memory);
        }
    }
}
