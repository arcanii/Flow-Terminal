//! One Flow per program and config folder on Windows
//! (`Platform::hand_to_running`, docs/design/put-away.md). The first start
//! holds the mutex `Local\Flow.Instance.<id>`. A later start finds it there,
//! sets the event `Flow.Again.<id>`, and waits up to `AGAIN_WAIT` for
//! `Flow.Forward.<id>`, which the running Flow sets once its window has
//! come forward; then it quits, and otherwise starts beside it. Each start
//! creates the events too (`CreateEventW` opens one that's there), so it
//! works whichever is made first. They're Local: the logon session's
//! alone.

use std::sync::Mutex;
use std::time::Duration;

use flow_core::event::UserEvent;
use flow_core::platform::Start;
use flow_core::session::Waker;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, ResetEvent, SetEvent, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};

/// How long a start waits for the running Flow to come forward, long
/// enough for one that's still starting to get to it.
pub const AGAIN_WAIT: Duration = Duration::from_secs(10);

/// The events of the start that runs, as numbers (a handle isn't `Send`).
#[derive(Clone, Copy)]
struct Held {
    again: usize,
    forward: usize,
}

/// This process's own, once it's the first start.
static HELD: Mutex<Option<Held>> = Mutex::new(None);

/// What a start found.
enum Claimed {
    /// None ran: this one holds the name, and its events where they could
    /// be made.
    First(Option<Held>),
    Handed,
    Beside,
}

pub fn hand_to_running(id: &str) -> Start {
    match claim(id, AGAIN_WAIT) {
        Claimed::First(held) => {
            *HELD.lock().unwrap_or_else(|e| e.into_inner()) = held;
            Start::First
        }
        Claimed::Handed => Start::Handed,
        Claimed::Beside => Start::Beside,
    }
}

/// Each later start a `UserEvent::StartedAgain`, on a thread of its own.
pub fn listen(waker: Waker) {
    let held = *HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(held) = held {
        wait_for_starts(held.again, move || waker.send(UserEvent::StartedAgain));
    }
}

/// Tells the start waiting that the window has come forward.
pub fn came_forward() {
    if let Some(held) = *HELD.lock().unwrap_or_else(|e| e.into_inner()) {
        // SAFETY: an event this process made and never closes.
        unsafe { SetEvent(held.forward as HANDLE) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn claim(id: &str, wait: Duration) -> Claimed {
    let name = |part: &str| wide(&format!("Local\\Flow.{part}.{id}"));
    // SAFETY: names that outlive the calls; no security attributes, so the
    // objects get the process's default, the user's.
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, name("Instance").as_ptr());
        // Read before any other call can change it.
        let running = GetLastError() == ERROR_ALREADY_EXISTS;
        let again = CreateEventW(std::ptr::null(), 0, 0, name("Again").as_ptr());
        let forward = CreateEventW(std::ptr::null(), 0, 0, name("Forward").as_ptr());
        let made = !again.is_null() && !forward.is_null();
        if mutex.is_null() || !running {
            // The mutex stays open as long as the process runs: it is the
            // claim. One that couldn't be made (its name taken by another
            // kind of object) leaves this Flow running as Flows always did.
            let held = (made && !mutex.is_null()).then_some(Held { again: again as usize, forward: forward as usize });
            return Claimed::First(held);
        }
        CloseHandle(mutex);
        if !made {
            return Claimed::Beside;
        }
        // An answer to a start that gave up waiting isn't this one's.
        ResetEvent(forward);
        // The user's click let this start come forward; the running Flow
        // may, in its place.
        AllowSetForegroundWindow(ASFW_ANY);
        SetEvent(again);
        let answered = WaitForSingleObject(forward, wait.as_millis() as u32) == WAIT_OBJECT_0;
        CloseHandle(again);
        CloseHandle(forward);
        if answered {
            Claimed::Handed
        } else {
            Claimed::Beside
        }
    }
}

/// Runs `started` for each start, on a thread that waits on the event.
fn wait_for_starts(again: usize, started: impl Fn() + Send + 'static) {
    let _ = std::thread::Builder::new().name("flow-again".into()).spawn(move || loop {
        // SAFETY: an event this process made and never closes.
        if unsafe { WaitForSingleObject(again as HANDLE, INFINITE) } != WAIT_OBJECT_0 {
            return;
        }
        started();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_id(what: &str) -> String {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        format!("test-{what}-{}-{now}", std::process::id())
    }

    #[test]
    fn a_second_start_is_handed_to_the_first_once_it_answers() {
        let id = scratch_id("handed");
        let Claimed::First(Some(held)) = claim(&id, AGAIN_WAIT) else { panic!("the first start holds the name") };
        let forward = held.forward;
        // SAFETY: the first start's own event, never closed.
        wait_for_starts(held.again, move || unsafe {
            SetEvent(forward as HANDLE);
        });
        assert!(matches!(claim(&id, Duration::from_secs(5)), Claimed::Handed));
        assert!(matches!(claim(&id, Duration::from_secs(5)), Claimed::Handed), "and each start after it");
    }

    #[test]
    fn a_start_that_isnt_answered_runs_beside() {
        let id = scratch_id("beside");
        let Claimed::First(Some(_)) = claim(&id, AGAIN_WAIT) else { panic!("the first start holds the name") };
        let began = std::time::Instant::now();
        assert!(matches!(claim(&id, Duration::from_millis(200)), Claimed::Beside));
        assert!(began.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn another_flow_has_a_name_of_its_own() {
        assert!(matches!(claim(&scratch_id("one"), AGAIN_WAIT), Claimed::First(Some(_))));
        assert!(matches!(claim(&scratch_id("other"), AGAIN_WAIT), Claimed::First(Some(_))));
    }
}
