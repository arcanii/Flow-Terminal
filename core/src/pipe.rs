//! A way for programs in a tab to send Flow images around the terminal. On
//! Windows, ConPTY sits between Flow and the shell and passes on only what
//! its console keeps, so kitty's image uploads never arrive. Each tab there
//! opens a named pipe and tells its programs the name in `FLOW_IMAGE_PIPE`.
//! What they write to it is read as if written to the terminal, but only for
//! images: graphics commands that send, ask about and delete pictures, and
//! the queries a program asks before it sends any (XTVERSION, a cell's and the
//! text area's size in pixels, and DA1, whose answer comes last). Answers go
//! back up the pipe. The placeholder cells that show the pictures still go
//! through the terminal, so pictures sent this way are virtual placements
//! (`U=1`). ConPTY drops sixels too, so they come down the pipe as well,
//! and Flow places each where ConPTY's cursor is (`session.rs`'s
//! `show_piped_sixel`). Elsewhere the terminal carries everything and
//! there's no pipe.

use std::io::{Read, Write};
use std::sync::{Arc, OnceLock};

use crate::platform::Platform;

/// The variable that names a tab's pipe, for the programs in it.
pub const VAR: &str = "FLOW_IMAGE_PIPE";

/// A program's connection to a tab's pipe.
pub trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

/// Takes a program that connected, on a thread of its own, until it hangs up.
pub type Serve = Arc<dyn Fn(Box<dyn Stream>) + Send + Sync>;

/// An open pipe. Dropping it stops it taking programs; those connected stay.
pub trait Pipe: Send {
    /// The name programs open it by.
    fn name(&self) -> &str;
}

static PLATFORM: OnceLock<&'static dyn Platform> = OnceLock::new();

/// Lets tabs open pipes through `platform` (`Platform::open_pipe`), once,
/// before shells start.
pub fn set_platform(platform: &'static dyn Platform) {
    let _ = PLATFORM.set(platform);
}

/// A new pipe that hands each program connecting to it to `serve`; None
/// where the platform has none.
pub(crate) fn open(serve: Serve) -> Option<Box<dyn Pipe>> {
    PLATFORM.get()?.open_pipe(serve)
}
