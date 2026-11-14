//! Windows session-wide instance ownership, shared by portable and installed builds.
//! The owning thread holds a named mutex for the application's entire lifetime.
use std::{ffi::c_void, marker::PhantomData, rc::Rc};
const MUTEX_NAME: &str = "Local\\OpenMouse.Desktop.Instance.v1";
const EVENT_NAME: &str = "Local\\OpenMouse.Desktop.Activate.v1";
const WINDOW_PROPERTY: &str = "OpenMouse.Desktop.MainWindow.v1";
const WAIT_OBJECT_0: u32 = 0;
const WAIT_ABANDONED: u32 = 0x80;
const WAIT_TIMEOUT: u32 = 258;
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(security: *const c_void, owner: i32, name: *const u16) -> isize;
    fn CreateEventW(
        security: *const c_void,
        manual_reset: i32,
        initial: i32,
        name: *const u16,
    ) -> isize;
    fn GetLastError() -> u32;
    fn SetLastError(error: u32);
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn ReleaseMutex(handle: isize) -> i32;
    fn SetEvent(handle: isize) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
    fn GetPropW(hwnd: isize, name: *const u16) -> isize;
    fn SetPropW(hwnd: isize, name: *const u16, value: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn AllowSetForegroundWindow(pid: u32) -> i32;
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn MessageBoxW(hwnd: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct Handle(isize);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub enum Launch {
    Primary(Instance),
    Secondary,
}
pub struct Instance {
    mutex: Handle,
    event: Handle,
    // A mutex must be released on its owning thread, never a spawned UI task.
    _thread: PhantomData<Rc<()>>,
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.mutex.0);
        }
    }
}
impl Instance {
    pub fn acquire() -> Result<Launch, String> {
        acquire_named(MUTEX_NAME, EVENT_NAME)
    }
    pub fn activation_event(&self) -> isize {
        self.event.0
    }
}
fn acquire_named(mutex_name: &str, event_name: &str) -> Result<Launch, String> {
    // Creating/opening the event first closes the simultaneous-launch race:
    // the losing process can signal it before the first window is ready.
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(event_name).as_ptr()) };
    if event == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let event = Handle(event);
    unsafe {
        SetLastError(0);
    }
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 1, wide(mutex_name).as_ptr()) };
    let existed = unsafe { GetLastError() } == 183;
    if mutex == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mutex = Handle(mutex);
    if existed {
        match unsafe { WaitForSingleObject(mutex.0, 0) } {
            WAIT_TIMEOUT => {
                allow_existing_window_to_activate();
                if unsafe { SetEvent(event.0) } == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                return Ok(Launch::Secondary);
            }
            WAIT_OBJECT_0 | WAIT_ABANDONED => {} // Previous instance exited or crashed.
            _ => return Err(std::io::Error::last_os_error().to_string()),
        }
    }
    Ok(Launch::Primary(Instance {
        mutex,
        event,
        _thread: PhantomData,
    }))
}
unsafe extern "system" fn allow_window(hwnd: isize, name: isize) -> i32 {
    if unsafe { GetPropW(hwnd, name as *const u16) } != 0 {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid != 0 {
            unsafe {
                AllowSetForegroundWindow(pid);
            }
        }
        return 0;
    }
    1
}
fn allow_existing_window_to_activate() {
    let name = wide(WINDOW_PROPERTY);
    unsafe {
        EnumWindows(allow_window, name.as_ptr() as isize);
    }
}
pub fn register_main_window(hwnd: isize) {
    if hwnd != 0 {
        unsafe {
            SetPropW(hwnd, wide(WINDOW_PROPERTY).as_ptr(), 1);
        }
    }
}
pub fn take_activation(event: isize) -> bool {
    (unsafe { WaitForSingleObject(event, 0) }) == WAIT_OBJECT_0
}
/// Call outside an App/Context update; foregrounding can dispatch Win32 messages.
pub fn show_existing_window(hwnd: isize) {
    unsafe {
        if hwnd != 0 && IsWindow(hwnd) != 0 {
            ShowWindow(hwnd, 9);
            SetForegroundWindow(hwnd);
        }
    }
}
pub fn show_startup_error(error: &str) {
    unsafe {
        MessageBoxW(
            0,
            wide(&format!(
                "OpenMouse could not verify single-instance ownership.\n\n{error}"
            ))
            .as_ptr(),
            wide("OpenMouse Desktop").as_ptr(),
            0x10,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_launch_signals_owner_and_exit_releases_ownership() {
        let tag = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mutex = format!("Local\\OpenMouse.Test.Instance.{tag}");
        let event = format!("Local\\OpenMouse.Test.Event.{tag}");
        let primary = match acquire_named(&mutex, &event).unwrap() {
            Launch::Primary(primary) => primary,
            _ => panic!("First launch must own the mutex"),
        };
        assert!(!take_activation(primary.activation_event()));
        let (other_mutex, other_event) = (mutex.clone(), event.clone());
        assert!(std::thread::spawn(move || matches!(
            acquire_named(&other_mutex, &other_event).unwrap(),
            Launch::Secondary
        ))
        .join()
        .unwrap());
        assert!(take_activation(primary.activation_event()));
        assert!(!take_activation(primary.activation_event()));
        drop(primary);
        assert!(matches!(
            acquire_named(&mutex, &event).unwrap(),
            Launch::Primary(_)
        ));
    }
}
