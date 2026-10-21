use std::ffi::CString;
use std::ptr;
use std::time::{Duration, Instant};
use windows::core::PCSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingA, UnmapViewOfFile, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows::Win32::System::Threading::{
    CreateEventA, CreateMutexA, OpenEventA, ReleaseMutex, SetEvent, WaitForSingleObject,
    EVENT_MODIFY_STATE,
};

#[repr(C)]
struct Header {
    max_size: u32,
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    resize: i32,
    mirror: i32,
    timeout: i32,
}
pub struct VirtualCamWriter {
    mapping: Option<HANDLE>,
    mutex: Option<HANDLE>,
    sent: Option<HANDLE>,
    want: Option<HANDLE>,
    view: *mut u8,
    last_attempt: Instant,
}
unsafe impl Send for VirtualCamWriter {}
impl VirtualCamWriter {
    pub fn new() -> Self {
        Self {
            mapping: None,
            mutex: None,
            sent: None,
            want: unsafe {
                CreateEventA(None, false, false, windows::core::s!("UnityCapture_Want")).ok()
            },
            view: ptr::null_mut(),
            last_attempt: Instant::now() - Duration::from_secs(2),
        }
    }
    fn open(&mut self) -> bool {
        if !self.view.is_null() {
            return true;
        }
        if self.last_attempt.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.last_attempt = Instant::now();
        unsafe {
            let name = |n: &str| CString::new(n).unwrap();
            let m = name("UnityCapture_Mutx");
            let Ok(mutex) = CreateMutexA(None, false, PCSTR(m.as_ptr() as _)) else {
                return false;
            };
            let e = name("UnityCapture_Sent");
            let Ok(sent) = OpenEventA(EVENT_MODIFY_STATE, false, PCSTR(e.as_ptr() as _)) else {
                let _ = CloseHandle(mutex);
                return false;
            };
            let d = name("UnityCapture_Data");
            let Ok(mapping) = OpenFileMappingA(FILE_MAP_WRITE.0, false, PCSTR(d.as_ptr() as _))
            else {
                let _ = CloseHandle(mutex);
                let _ = CloseHandle(sent);
                return false;
            };
            let view = MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(mapping);
                let _ = CloseHandle(mutex);
                let _ = CloseHandle(sent);
                return false;
            }
            self.mapping = Some(mapping);
            self.mutex = Some(mutex);
            self.sent = Some(sent);
            self.view = view.Value as _;
            true
        }
    }
    pub fn write_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> bool {
        if !self.open() {
            return false;
        }
        unsafe {
            let wait = WaitForSingleObject(self.mutex.unwrap(), 5);
            if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
                return false;
            }
            let header = self.view as *mut Header;
            let size = (width as usize) * (height as usize) * 4;
            let valid = size <= (*header).max_size as usize && rgba.len() >= size;
            if valid {
                (*header).width = width as i32;
                (*header).height = height as i32;
                // UnityCapture stride is measured in pixels.
                (*header).stride = width as i32;
                (*header).format = 0;
                (*header).resize = 1;
                (*header).mirror = 0;
                (*header).timeout = 1000;
                // UnityCapture's RGB bitmaps are bottom-up; decoded images and
                // the desktop/phone previews are top-down.
                let stride = width as usize * 4;
                let target = self.view.add(std::mem::size_of::<Header>());
                for row in 0..height as usize {
                    ptr::copy_nonoverlapping(
                        rgba.as_ptr().add((height as usize - 1 - row) * stride),
                        target.add(row * stride),
                        stride,
                    );
                }
            }
            let _ = ReleaseMutex(self.mutex.unwrap());
            if valid {
                let _ = SetEvent(self.sent.unwrap());
            }
            // Only report an active consumer when the filter has requested a frame.
            valid
                && self
                    .want
                    .map(|h| WaitForSingleObject(h, 0) == WAIT_OBJECT_0)
                    .unwrap_or(false)
        }
    }
}
impl Drop for VirtualCamWriter {
    fn drop(&mut self) {
        unsafe {
            if !self.view.is_null() {
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view as _,
                });
            }
            for h in [self.mapping, self.mutex, self.sent, self.want]
                .into_iter()
                .flatten()
            {
                let _ = CloseHandle(h);
            }
        }
    }
}
