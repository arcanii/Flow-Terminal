# A theme's window frame

The Flow notanAmiga CLI's shell window, drawn round each pane that wears the
theme ([Themes](../manual/themes.md)' Theme details, **Its window frame**, `ui.look_frame`). It
was asked for on 2026-11-10, with a picture of the shell's window on its
desktop, and came in 0.1.36. On 2026-11-17 it was redrawn after a picture
of the real AmigaShell window under Kickstart 1.3 in WinUAE, to look as
that one does.

## What's drawn

Everything is counted in the machine's own units:
- **a line**, a tenth of the title bar (`Frame::unit`);
- **a pixel**, half a line across.

At a scale of 1 and the font's 16 pixels, those units are the screen's own. The bar is
20 pixels tall, as in the picture.

- **The outline**, in the pane's text color, the machine's white.
- **The title bar**, white, with the background's color, the machine's blue,
  between its parts. From left to right:
  - **Flow's close box.** It is white with a cross in blue. It closes that pane,
    or its tab where it's the only one, as ⌘W does. The machine's shell window
    had no close gadget; this one was chosen in 0.1.36.
  - **The tab's name**, in the pane's own letters. The letters are in blue in a
    box of white.
  - **Three white lines** to the gadgets: the bar's top, its middle and its
    foot. Each is two lines thick, and they are two lines apart. The frame
    drew four until 2026-11-17.
  - **The two depth gadgets**, 22 pixels wide, a gap apart and a gap from
    the outline. They leave out where the pane is too narrow for them.
    - The first is a box in outline with a black one, the window, in front
      of it. It brings Flow's window in front of every other window
      (`Platform::bring_forward`).
    - The second is the black box behind one in outline. It sends Flow's
      window behind every other window, and the keys go to the window then
      in front (`Platform::send_to_back`). A window dropped down by the
      hotkey is put away instead, as the hotkey puts it away.
    - These were first to maximize and restore. On 2026-11-17 they were
      made to do what the AmigaOS manual says of the depth gadget instead.
      The order, front then back, is read from the pictures (the black box
      in front, then behind). The manual's later single depth gadget
      toggles; 1.3's two each do one thing.
    - Windows sends the window back as Alt+Esc does. The Mac orders it to
      the back (`orderBack:`) and gives the keys to the app whose window
      is then in front, found in CoreGraphics' list of the windows on
      screen (`CGWindowListCopyWindowInfo`: the first at the normal level
      that isn't Flow's), yielding to it on macOS 14 and later. Linux
      does it on X11, XWayland's too (toward 0.1.44.1, the back gadget
      built first): Flow asks the window manager to lower
      it as a pager does (`_NET_RESTACK_WINDOW`, which GNOME's mutter
      honors) and as a window does (ConfigureWindow's stack mode), then
      gives the keys to the window the manager stacks in front
      (`_NET_CLIENT_LIST_STACKING`). Under XWayland that list has none of
      the Wayland windows, so there the keys are the manager's to give.
      Wayland lets no program lower its window, so there `sends_to_back`
      is false and the gadget doesn't act.
- **The sizing gadget**, in the bottom right corner. It is 16 pixels by 9
  lines, over the outline, and shows a small box and a larger one in outline.
  - Dragging it drags the window's bottom right corner, through winit's
    `drag_resize_window`. On the Mac, where winit can't start that, Flow
    drags it itself: the window's size at the press, grown by how far the
    pointer has moved on the screen since (the window's place added to
    the pointer's), so the corner follows the pointer however AppKit
    places the window as it grows, never smaller than Flow's least window.
    It was found doing nothing in 0.1.44 on the Mac (2026-11-17).
  - It does nothing while the window is maximized or full screen. The
    pointer is the resize arrow only where it works, and all through
    Flow's own drag.
  - The text ends a line above it, so it never covers a letter.

The gadgets' pictures are tables of pieces in the machine's pixels: `TO_FRONT`,
`TO_BACK` and `SIZING` in `ui.rs`. A test paints them and holds them to the
picture's pixels.

## How it acts

- **A gadget under the pointer** that would do something is drawn as the
  machine drew one pressed:
  - its colors are turned over, white and black swapping;
  - its blue becomes the cursor's orange.
  - The back gadget does this only where it works, and so does the sizing
    gadget.
- **A frame whose pane hasn't the keys**, or every frame while Flow hasn't, is
  ghosted, as the machine ghosted its inactive windows' bars:
  - a dot of white in every four pixels;
  - two pixels further along on every other line;
  - over the name and the gaps between the lines, not the gadgets.
- **Screen readers** get the close box alone. The system arranges windows for
  them its own way, and a reader can't drag.

## Your own and saved themes (0.1.48)

Decided 2026-11-27: a theme of your own, or one saved under a name, may
wear the frame too, rather than only carrying the one its machine had.

- **Kept** as `[theme] frame` (`ThemeConfig::frame`, off by default), so a
  saved theme keeps it in its own `theme`, a tab's own look carries it
  (`looks::TAB_KEYS`), and a preset file moves it with the rest.
- **Begun** from the machine your own theme began from, as Theme details
  left it (`Look::as_theme`, `looks::wear` with `ui.look_frame`), and a
  machine's theme saved under a name keeps its frame the same way
  (`looks::save_as`).
- **Switched** by a **Window frame** row among your own theme's rows under
  Theme (`SettingId::OwnFrame`, `settings::own_rows`), which lays every pane
  out again as Its window frame does. Theme details' Its window frame is a
  machine's switch alone; your own theme has the one row.
- **Worn**: `looks::framed` asks the look in use (`InUse`): a machine's (or
  a package's) frame with Theme details' switch on, else the theme's own.
- **Exported**: Export… writes it as the package's `frame`
  (`packaged::export`), so `flowapp install` of it elsewhere frames it there.
- **flow-core-themes 0.1.1** carries the notanAmiga's `frame` explicitly
  (`examples/theme_package.rs --version 0.1.1`), packed on the Mac; it's
  signed and published only on the user's word. Flow-SDK's theme entry
  writes `frame` from its next version.
- **Tried** on the Mac (2026-11-27): the tests carry it through your own
  theme, a saved one, a tab's own look and an exported package, and a
  snapshot of your own theme with it on drew the frame round its pane. Not
  yet in a live window.

## Left out

- The machine's own close gadget.
- Sending the window back on Wayland, which lets no program do it.
