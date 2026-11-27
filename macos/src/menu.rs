//! The menu bar and the terminal's context menu.
//!
//! AppKit runs menu actions on the main thread, outside winit's event
//! handler, so they are forwarded as `UserEvent::Command` through the event
//! loop proxy and handled like any other event.

use std::cell::{Cell, OnceCell, RefCell};

use flow_core::event::{Command, UserEvent};
use flow_core::i18n::{t, tf};
use flow_core::platform::MenuState;
use flow_core::session::Waker;
use flow_core::APP_NAME;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu, NSMenuItem, NSView,
    NSWorkspace,
};
use objc2_foundation::{NSObjectNSThreadPerformAdditions, NSObjectProtocol, NSPoint, NSString, NSURL};
use winit::window::Window;

use crate::window::ns_view;

/// Where "Flow Releases" in the Help menu goes.
const RELEASES_URL: &str = "https://github.com/arcanii/Flow-releases/releases";

/// A context menu waiting to open: the menu, where, and in which view.
type PendingMenu = (Retained<NSMenu>, NSPoint, Retained<NSView>);

thread_local! {
    static TARGET: OnceCell<Retained<MenuTarget>> = const { OnceCell::new() };
    static SINK: RefCell<Option<Waker>> = const { RefCell::new(None) };
    static STATE: Cell<MenuState> =
        const { Cell::new(MenuState { auto_mode: false, has_key: false, has_selection: false, tabs: 1, efficient: false, on_top: false, recording: false, capturing: false }) };
    static PENDING: RefCell<Option<PendingMenu>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and MenuTarget has no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FlowMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(runCommand:))]
        fn run_command(&self, sender: &NSMenuItem) {
            if let Some(cmd) = command_for_tag(sender.tag()) {
                SINK.with(|s| {
                    if let Some(waker) = s.borrow().as_ref() {
                        waker.send(UserEvent::Command(cmd));
                    }
                });
            }
        }

        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let state = STATE.with(Cell::get);
            let check = |on: bool| {
                item.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
                true
            };
            match command_for_tag(item.tag()) {
                // While Settings waits for a key to bind, or to be the
                // hotkey, every item gives its key up, so ⌘T can be pressed.
                Some(_) if state.capturing => false,
                Some(Command::GuideMode) => check(!state.auto_mode),
                Some(Command::AutoMode) => check(state.auto_mode),
                Some(Command::ToggleEfficient) => check(state.efficient),
                Some(Command::ToggleOnTop) => check(state.on_top),
                Some(Command::Record) => check(state.recording),
                Some(Command::NextTab | Command::PreviousTab) => state.tabs > 1,
                Some(Command::Copy) => state.has_selection,
                Some(Command::ForgetKey) => state.has_key,
                // Named by the provider in use, which may have changed
                // since the menu was made.
                Some(Command::ToggleAssistant | Command::OpenAssistant) => {
                    item.setTitle(&NSString::from_str(&ask_words()));
                    true
                }
                _ => true,
            }
        }

        #[unsafe(method(showContextMenu:))]
        fn show_context_menu(&self, _arg: Option<&AnyObject>) {
            if let Some((menu, at, view)) = PENDING.with(|p| p.borrow_mut().take()) {
                menu.popUpMenuPositioningItem_atLocation_inView(None, at, Some(&view));
            }
        }

        #[unsafe(method(openReleases:))]
        fn open_releases(&self, _sender: Option<&AnyObject>) {
            open_url(RELEASES_URL);
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

/// The Ask item's words, the assistant named by the provider in use:
/// Ask Claude, Ask GPT or Ask Gemini.
fn ask_words() -> String {
    tf("menu.ask", &[("ai", &flow_core::ai::provider::in_use().name())])
}

/// Opens `url` in the default browser; false if it isn't one.
pub(crate) fn open_url(url: &str) -> bool {
    NSURL::URLWithString(&NSString::from_str(url)).is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
}

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn shared(mtm: MainThreadMarker) -> Retained<Self> {
        TARGET.with(|t| t.get_or_init(|| Self::new(mtm)).clone())
    }
}

/// Commands without an argument, in tag order (tag = index + 1).
const COMMANDS: [Command; 42] = [
    Command::About,
    Command::CheckForUpdates,
    Command::Settings,
    Command::Quit,
    Command::Copy,
    Command::Paste,
    Command::SelectAll,
    Command::ClearBuffer,
    Command::BiggerText,
    Command::SmallerText,
    Command::ActualSize,
    Command::ToggleAssistant,
    Command::OpenAssistant,
    Command::NewConversation,
    Command::GuideMode,
    Command::AutoMode,
    Command::ChangeKey,
    Command::ForgetKey,
    Command::NewTab,
    Command::CloseTab,
    Command::NextTab,
    Command::PreviousTab,
    Command::SearchHistory,
    Command::Help,
    Command::Restart,
    Command::RestartTab,
    Command::ToggleEfficient,
    Command::Record,
    Command::SaveTheme,
    Command::Find,
    Command::FindNext,
    Command::FindPrevious,
    Command::Palette,
    Command::AskWhy,
    Command::SplitRight,
    Command::SplitDown,
    Command::ScrollToBottom,
    Command::ScrollToTop,
    Command::PreviousCommand,
    Command::NextCommand,
    Command::ToggleOnTop,
    Command::RenameTab,
];
/// `Command::NewTabWith(i)` is tagged `SHELL_TAG + i`, and
/// `Command::HelpPage(i)` `HELP_TAG + i`.
const SHELL_TAG: isize = 1000;
const HELP_TAG: isize = 2000;

fn tag_for(cmd: Command) -> isize {
    match cmd {
        Command::NewTabWith(i) => SHELL_TAG + i as isize,
        Command::HelpPage(i) => HELP_TAG + i as isize,
        cmd => COMMANDS.iter().position(|&c| c == cmd).map_or(0, |i| i as isize + 1),
    }
}

/// Hands an event to the app, as the menu's commands are.
pub fn send(event: UserEvent) {
    SINK.with(|s| {
        if let Some(waker) = s.borrow().as_ref() {
            waker.send(event);
        }
    });
}

fn command_for_tag(tag: isize) -> Option<Command> {
    if tag >= HELP_TAG {
        Some(Command::HelpPage((tag - HELP_TAG) as usize))
    } else if tag >= SHELL_TAG {
        Some(Command::NewTabWith((tag - SHELL_TAG) as usize))
    } else if tag >= 1 {
        COMMANDS.get(tag as usize - 1).copied()
    } else {
        None
    }
}

/// A chord as an item's key equivalent: the character (AppKit's own for
/// the named keys, as NSF3FunctionKey) and the modifiers.
fn equivalent(chord: &flow_core::keys::Chord) -> (String, NSEventModifierFlags) {
    use flow_core::keys::{Key, Named};
    let key = match chord.key {
        Key::Char(c) => c.to_string(),
        Key::Named(named) => {
            let c = match named {
                Named::Up => '\u{F700}',
                Named::Down => '\u{F701}',
                Named::Left => '\u{F702}',
                Named::Right => '\u{F703}',
                Named::F(n) => char::from_u32(0xF703 + n as u32).unwrap_or(' '),
                Named::Insert => '\u{F727}',
                Named::Delete => '\u{F728}',
                Named::Home => '\u{F729}',
                Named::End => '\u{F72B}',
                Named::PageUp => '\u{F72C}',
                Named::PageDown => '\u{F72D}',
                Named::Backspace => '\u{8}',
                Named::Tab => '\t',
                Named::Enter => '\r',
                Named::Space => ' ',
                Named::Escape => '\u{1b}',
            };
            c.to_string()
        }
    };
    let m = chord.mods;
    let held = [(m.ctrl, NSEventModifierFlags::Control), (m.alt, NSEventModifierFlags::Option), (m.shift, NSEventModifierFlags::Shift), (m.cmd, NSEventModifierFlags::Command)];
    let mods = held.iter().filter(|(on, _)| *on).fold(NONE, |all, (_, flag)| all | *flag);
    (key, mods)
}

const NONE: NSEventModifierFlags = NSEventModifierFlags(0);
const CMD: NSEventModifierFlags = NSEventModifierFlags::Command;

/// Builds menus whose command items all go to the shared target.
struct Menus {
    mtm: MainThreadMarker,
    target: Retained<MenuTarget>,
}

impl Menus {
    fn new(mtm: MainThreadMarker) -> Self {
        Self { mtm, target: MenuTarget::shared(mtm) }
    }

    /// A menu in the menu bar.
    fn submenu(&self, bar: &NSMenu, title: &str) -> Retained<NSMenu> {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(self.mtm), &NSString::from_str(title));
        let item = NSMenuItem::new(self.mtm);
        item.setSubmenu(Some(&menu));
        bar.addItem(&item);
        menu
    }

    /// An item that sends `cmd` to the app.
    fn command(&self, menu: &NSMenu, title: &str, cmd: Command, key: &str, mods: NSEventModifierFlags) -> Retained<NSMenuItem> {
        let item = self.item(menu, title, sel!(runCommand:), key, mods);
        self.target_self(&item);
        item.setTag(tag_for(cmd));
        item
    }

    /// An item that sends `cmd` to the app, with its keys as they're bound
    /// (`flow_core::keys`): the first shown, the rest taken by hidden
    /// items, as ⌘= beside ⌘+.
    fn bound(&self, menu: &NSMenu, title: &str, cmd: Command) -> Retained<NSMenuItem> {
        let bound = flow_core::keys::bound();
        let chords = bound.chords(cmd);
        let Some((first, rest)) = chords.split_first() else { return self.command(menu, title, cmd, "", NONE) };
        let (key, mods) = equivalent(first);
        let item = self.command(menu, title, cmd, &key, mods);
        for chord in rest {
            let (key, mods) = equivalent(chord);
            let alias = self.command(menu, title, cmd, &key, mods);
            alias.setHidden(true);
            alias.setAllowsKeyEquivalentWhenHidden(true);
        }
        item
    }

    /// An item AppKit handles itself, through the responder chain.
    fn system(&self, menu: &NSMenu, title: &str, action: Sel, key: &str, mods: NSEventModifierFlags) -> Retained<NSMenuItem> {
        self.item(menu, title, action, key, mods)
    }

    fn item(&self, menu: &NSMenu, title: &str, action: Sel, key: &str, mods: NSEventModifierFlags) -> Retained<NSMenuItem> {
        // SAFETY: every action used here is a real selector of NSApplication,
        // NSWindow or MenuTarget.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        if !key.is_empty() {
            item.setKeyEquivalentModifierMask(mods);
        }
        menu.addItem(&item);
        item
    }

    fn target_self(&self, item: &NSMenuItem) {
        let target: &AnyObject = &self.target;
        // SAFETY: the target lives in TARGET for the rest of the process.
        unsafe { item.setTarget(Some(target)) };
    }

    fn separator(&self, menu: &NSMenu) {
        menu.addItem(&NSMenuItem::separatorItem(self.mtm));
    }
}

/// Replaces winit's minimal menu bar with Flow's, in the language in use;
/// called again when that changes. `shells` are the shell profile labels, in
/// order.
pub fn install_menu_bar(waker: Waker, shells: &[String], can_update: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    SINK.with(|s| *s.borrow_mut() = Some(waker));
    let m = Menus::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    let bar = NSMenu::new(mtm);

    // The first menu's title is always shown as the app name.
    let app_name: [(&str, &dyn std::fmt::Display); 1] = [("app", &APP_NAME)];
    let menu = m.submenu(&bar, APP_NAME);
    m.bound(&menu, &tf("menu.about", &app_name), Command::About);
    if can_update {
        m.bound(&menu, t("menu.check_updates"), Command::CheckForUpdates);
    }
    m.separator(&menu);
    m.bound(&menu, t("menu.settings"), Command::Settings);
    m.separator(&menu);
    let services = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(t("menu.services")));
    m.system(&menu, t("menu.services"), sel!(submenuAction:), "", NONE).setSubmenu(Some(&services));
    app.setServicesMenu(Some(&services));
    m.separator(&menu);
    m.system(&menu, &tf("menu.hide", &app_name), sel!(hide:), "h", CMD);
    m.system(&menu, t("menu.hide_others"), sel!(hideOtherApplications:), "h", CMD | NSEventModifierFlags::Option);
    m.system(&menu, t("menu.show_all"), sel!(unhideAllApplications:), "", NONE);
    m.separator(&menu);
    // ⇧⌘B restarts the tab in front (the Shell menu's Restart Tab); this
    // one, all of Flow, has no keys of its own.
    m.bound(&menu, &tf("menu.restart", &app_name), Command::Restart);
    m.bound(&menu, &tf("menu.quit", &app_name), Command::Quit);

    // Tabs, as in Terminal: ⌘1…⌘9 pick one (handled with the keys, not here).
    let menu = m.submenu(&bar, t("menu.shell"));
    m.bound(&menu, t("menu.new_tab"), Command::NewTab);
    for (i, label) in shells.iter().enumerate() {
        m.command(&menu, &tf("menu.new_shell_tab", &[("shell", label)]), Command::NewTabWith(i), "", NONE);
    }
    m.separator(&menu);
    m.bound(&menu, t("menu.previous_tab"), Command::PreviousTab);
    m.bound(&menu, t("menu.next_tab"), Command::NextTab);
    m.separator(&menu);
    m.bound(&menu, t("menu.split_right"), Command::SplitRight);
    m.bound(&menu, t("menu.split_down"), Command::SplitDown);
    m.separator(&menu);
    m.bound(&menu, t("menu.rename_tab"), Command::RenameTab);
    m.bound(&menu, t("menu.restart_tab"), Command::RestartTab);
    m.bound(&menu, t("menu.close_tab"), Command::CloseTab);
    m.system(&menu, t("menu.close_window"), sel!(performClose:), "w", CMD | NSEventModifierFlags::Shift);

    let menu = m.submenu(&bar, t("menu.edit"));
    m.bound(&menu, t("menu.copy"), Command::Copy);
    m.bound(&menu, t("menu.paste"), Command::Paste);
    m.bound(&menu, t("menu.select_all"), Command::SelectAll);
    m.separator(&menu);
    m.bound(&menu, t("menu.clear_buffer"), Command::ClearBuffer);
    m.bound(&menu, t("menu.search_history"), Command::SearchHistory);
    m.separator(&menu);
    m.bound(&menu, t("menu.find"), Command::Find);
    m.bound(&menu, t("menu.find_next"), Command::FindNext);
    m.bound(&menu, t("menu.find_previous"), Command::FindPrevious);
    m.separator(&menu);
    // The system's own picker; what it inserts comes back through insert.rs.
    m.system(&menu, t("menu.emoji"), sel!(orderFrontCharacterPalette:), " ", CMD | NSEventModifierFlags::Control);

    let menu = m.submenu(&bar, t("menu.view"));
    // ⌘= works too, without showing a second item.
    m.bound(&menu, t("menu.bigger"), Command::BiggerText);
    m.bound(&menu, t("menu.smaller"), Command::SmallerText);
    m.bound(&menu, t("menu.actual_size"), Command::ActualSize);
    m.separator(&menu);
    m.bound(&menu, t("menu.scroll_to_top"), Command::ScrollToTop);
    m.bound(&menu, t("menu.scroll_to_bottom"), Command::ScrollToBottom);
    m.bound(&menu, t("menu.previous_command"), Command::PreviousCommand);
    m.bound(&menu, t("menu.next_command"), Command::NextCommand);
    m.separator(&menu);
    m.bound(&menu, t("menu.palette"), Command::Palette);
    m.bound(&menu, t("menu.efficient"), Command::ToggleEfficient);
    m.bound(&menu, t("menu.record"), Command::Record);
    m.bound(&menu, t("menu.save_theme"), Command::SaveTheme);
    m.separator(&menu);
    m.system(&menu, t("menu.full_screen"), sel!(toggleFullScreen:), "f", CMD | NSEventModifierFlags::Control);

    let menu = m.submenu(&bar, t("menu.assistant"));
    m.bound(&menu, &ask_words(), Command::ToggleAssistant);
    m.bound(&menu, t("menu.new_conversation"), Command::NewConversation);
    m.bound(&menu, t("menu.ask_why"), Command::AskWhy);
    m.separator(&menu);
    m.bound(&menu, t("menu.guide_mode"), Command::GuideMode);
    m.bound(&menu, t("menu.auto_mode"), Command::AutoMode);
    m.separator(&menu);
    m.bound(&menu, t("menu.change_key"), Command::ChangeKey);
    m.bound(&menu, t("menu.remove_key"), Command::ForgetKey);

    let menu = m.submenu(&bar, t("menu.window"));
    m.system(&menu, t("menu.minimize"), sel!(performMiniaturize:), "m", CMD);
    m.system(&menu, t("menu.zoom"), sel!(performZoom:), "", NONE);
    m.bound(&menu, t("menu.on_top"), Command::ToggleOnTop);
    m.separator(&menu);
    m.system(&menu, t("menu.bring_all"), sel!(arrangeInFront:), "", NONE);
    app.setWindowsMenu(Some(&menu));

    let menu = m.submenu(&bar, t("menu.help"));
    m.bound(&menu, &tf("menu.help_item", &app_name), Command::Help);
    m.separator(&menu);
    // Each screen's page, whatever's in front.
    for (i, page) in flow_core::help::Page::ALL.iter().enumerate() {
        m.command(&menu, page.title(), Command::HelpPage(i), "", NONE);
    }
    m.separator(&menu);
    let item = m.system(&menu, &tf("menu.releases", &app_name), sel!(openReleases:), "", NONE);
    m.target_self(&item);
    app.setHelpMenu(Some(&menu));

    app.setMainMenu(Some(&bar));
}

/// Keeps the menus' checkmarks and enabled items in step with the app.
pub fn set_menu_state(state: MenuState) {
    STATE.with(|s| s.set(state));
}

/// Opens the terminal's context menu at `at` (window pixels).
pub fn show_context_menu(window: &Window, at: (f32, f32), has_selection: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Some(view) = ns_view(window) else { return false };
    let m = Menus::new(mtm);
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    m.bound(&menu, t("menu.copy"), Command::Copy).setEnabled(has_selection);
    m.bound(&menu, t("menu.paste"), Command::Paste);
    m.bound(&menu, t("menu.select_all"), Command::SelectAll);
    m.separator(&menu);
    m.bound(&menu, t("menu.clear_buffer"), Command::ClearBuffer);
    m.bound(&menu, t("menu.search_history"), Command::SearchHistory);
    m.separator(&menu);
    m.bound(&menu, t("menu.find"), Command::Find);
    m.bound(&menu, t("menu.find_next"), Command::FindNext);
    m.bound(&menu, t("menu.find_previous"), Command::FindPrevious);
    m.separator(&menu);
    m.command(&menu, &ask_words(), Command::OpenAssistant, "", NONE);

    let scale = window.scale_factor();
    let (x, y) = (at.0 as f64 / scale, at.1 as f64 / scale);
    let y = if view.isFlipped() { y } else { view.bounds().size.height - y };
    PENDING.with(|p| *p.borrow_mut() = Some((menu, NSPoint::new(x, y), view)));
    // The menu runs its own loop until it closes. Opening it after the
    // current event is handled keeps the liquid and the terminal drawing.
    let target: &NSObject = &m.target;
    // SAFETY: showContextMenu: takes an optional object and ignores it.
    unsafe { target.performSelectorOnMainThread_withObject_waitUntilDone(sel!(showContextMenu:), None, false) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_tags_round_trip() {
        for cmd in COMMANDS.into_iter().chain([Command::NewTabWith(0), Command::NewTabWith(7)]) {
            assert_eq!(command_for_tag(tag_for(cmd)), Some(cmd));
        }
        assert_eq!(command_for_tag(0), None);
    }

    #[test]
    fn a_bound_key_is_the_menu_items_key_equivalent() {
        use flow_core::keys::Chord;
        let of = |text: &str| equivalent(&Chord::parse(text).unwrap());
        assert_eq!(of("cmd+t"), ("t".to_string(), CMD));
        assert_eq!(of("cmd+shift+g"), ("g".to_string(), CMD | NSEventModifierFlags::Shift));
        assert_eq!(of("cmd+plus"), ("+".to_string(), CMD));
        assert_eq!(of("ctrl+space"), (" ".to_string(), NSEventModifierFlags::Control));
        // AppKit's characters for the named keys: NSF3FunctionKey, NSEndFunctionKey.
        assert_eq!(of("f3"), ("\u{F706}".to_string(), NONE));
        assert_eq!(of("cmd+end"), ("\u{F72B}".to_string(), CMD));
        assert_eq!(of("alt+cmd+left"), ("\u{F702}".to_string(), CMD | NSEventModifierFlags::Option));
    }
}
