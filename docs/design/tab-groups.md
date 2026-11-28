# Tabs in groups, in a sidebar

Asked for on 2026-11-27, with a picture of another app's sidebar: a
panel that pops out from the window's left edge, listing the tabs, which
can be put into groups, the groups kept between launches. In the picture
each group has a name and a colored mark and folds open and shut. Under
it are its tabs, each with a name and a line saying what it's doing: a
command's tests passed with a ✓, a server's `localhost:8765` with a dot,
or a git branch and how many files it has changed. The tab in front is
lit, a tab can fold open to show what's under it, and the foot has
buttons for a new tab and a new group.

Decided on 2026-11-28:

- **Where it goes:** it pops out over the terminal, and a pin keeps it
  out, taking its room from the terminal then.
- **The title bar** keeps every tab, as now, each marked with its
  group's color.
- **What a group does:** it holds its tabs, folds open and shut, closes
  whole, and may wear a theme that its tabs wear unless they have their
  own.
- **An empty group goes:** groups live in `tabs.json` with their tabs,
  so they come back whenever the tabs do, and a group goes when its last
  tab closes or leaves it. No file of their own.

## What it builds on

- **The tabs** are `App::tabs`, drawn in the title bar by `ui::draw_tabs`
  as pills with a × (`Hit::Tab`, `Hit::CloseTab`), then the + and the …
  of the new-tab picker. Each `Tab` has its panes (`panes::Split`), its
  theme (`look`, `own`) and its alert glow. Tabs can't be dragged into
  another order today.
- **A tab's name** is its shell's, numbered where two run the same one
  (`tab_names`: zsh 1, zsh 2), with ` · <theme>` after it where the tab
  wears a theme of its own (`tab_labels`). It can't be named.
- **Between launches** `tabs.json` keeps the tabs (`config::OpenTabs`,
  `OpenTab`, a split tab's other panes in `more`), written as Flow quits
  and reopened where `shell.reopen_tabs` says. serde passes over keys it
  doesn't know, so an older Flow reads a newer file without its groups.
- **The layout:** `Layout::compute` lays out the title bar, the terminal
  and the assistant's panel, which takes its room from the window's foot,
  not a side (the backlog had that wrong). Nothing takes room from the
  left yet.
- **What a tab is doing** is mostly known already:
  - the command running, from the shell's marks (`ShellState::command`,
    `at_prompt`);
  - how the last one ended (`Finished`'s exit and time, `marks.rs`'s
    done and failed);
  - its folder (`Session::cwd`, from OSC 7 and 633's `Cwd`);
  - a program's progress (`ShellState::progress`, OSC 9;4);
  - an alert (`Tab::alert`, the title bar's glow);
  - the assistant's pane and an outside app's (`PaneFor`).
- **The card under a tab** (`ui::draw_tab_card`, `TAB_CARD_WAIT`) shows
  what its programs use, read on the `flow-usage` thread.

## The groups

A new module, `core/src/tab_groups.rs`, holds them and the rules for the
tabs' order, apart from `App` so it's tested without a window:

- **A group** is an id, a name, a color (one of `GROUP_COLORS`, eight,
  handed out in turn), whether it's folded, and its theme (a `looks::ids`
  id; none, Settings' theme).
- **Each `Tab`** has `group: Option<u64>`, the group's id, and `name:
  Option<String>`, a name the user gave it.
- **The order:** `App::tabs` stays the one order every key, the title
  bar and the sidebar go by. A group's tabs are always together in it,
  the groups in the sidebar's order, and tabs in no group (loose tabs)
  may stand before, between or after groups, as in a browser.
  `TabGroups` keeps that true: moving a tab into a group moves it to the
  end of the group's run, and moving one out puts it just after the run.
- **A group goes** once no tab is in it: its last tab closed, moved out
  or dragged into another group.
- **New tabs:**
  - ⌘T (Ctrl+Shift+T), the + and the new-tab picker open the tab at the
    end of the group of the tab in front, or at the end where that tab
    is loose.
  - The flow command's tab already opens right after the tab that asked,
    so it joins that tab's group.
  - Restart Tab keeps the tab's group and name.
- **New group** (the sidebar's foot, the palette, the Mac's Shell menu,
  `Command::NewGroup`, no key at first) puts the tab in front into a new
  group named `Group 1`, `Group 2` and so on, its name ready to be typed.
  So there's never a group with no tabs.
- **Ungroup** leaves its tabs where they are, loose.

As built in step 2 (2026-11-28, `core/src/tab_groups.rs` and
`core/src/app/groups.rs`):

- **A group** is its id, name and color (`tab_groups::Group`); it folds
  and wears a theme from steps 3 and 6. `TabGroups` holds them and its
  moves (`new_group`, `join`, `leave`, `settle`), which work on any list
  of tabs that say their group (`Grouped`), so they're tested apart, a
  long run of random moves among them.
- **The colors** are `tab_groups::COLORS`, eight, by name: blue, green,
  yellow, pink, teal, orange, purple and red. A new group takes the first
  that the fewest groups wear, so one given up comes back first.
- **New tabs:** `App::open_tab_in` (⌘T, the +, the new-tab picker, the
  palette's new tab in a shell, an update's and an app's hello tab)
  joins the group of the tab in front (`join_front_group`). The flow
  command's tab takes the group of the tab that asked, beside it. An
  outside app's tab goes at the end, loose. Restart Tab replaces the
  pane in place, so its tab keeps its group and name.
- **The palette** offers Move Tab to Group: *name* for each group but
  the front tab's own, **Remove Tab from Group** for a tab in one (the
  design had Ungroup in the tab's menu alone, and without the sidebar
  nothing else could take a tab out), and Go to Tab: *name* for each
  tab, its group's name beside it. None of them is kept among what was
  chosen lately (their `Item::key` is empty), since tabs' and groups'
  ids are new each run. A toast says where the tab in front went.
- **Names:** a new group is named `Group 1`, `Group 2` (`tab_group.name`),
  the first number no group has. Typing a group's name waits for the
  sidebar, where its header is (step 3).
- **`tabs.json`:** `OpenGroup` is a group's name and its color's name;
  each `OpenTab`'s `group` its place among them. Read back, a group's name
  is held as a tab's is, an unknown color takes the next in turn, and a
  tab whose group's tabs ended before it is loose (`together`), so a file
  written by hand still keeps each group's tabs together.
- **Readers** hear each tab's group after its name (`access.in_group`).

### A group's theme

A tab that has no theme of its own wears its group's, so the chain is:

1. the pane's own look (`own`);
2. its theme (`look`);
3. its group's theme;
4. Settings' theme.

`App::tab_config` and `pane_config` hand `tab_wearing` the group's theme
where the tab's `look` is none. A tab given a theme in the new-tab picker
keeps it, whatever its group wears.

- The title bar's ` · <theme>` names only a tab's own theme, not the
  group's, which the color mark already says.
- Settings on a tab that wears its group's theme changes that tab alone,
  as now: its changes become its own look.
- The group's menu picks the theme from the list the new-tab picker
  shows, and None takes it off.

As built in step 6 (2026-11-28):

- **The chain** is `App::worn_look` through `look_with_group` (tested
  alone): a pane's own `look` or `own`, else its group's theme
  (`Group::theme`, `TabGroups::set_theme`), which `tab_config`,
  `pane_config` and `fit_look` hand `tab_wearing`, Settings' theme last.
  A tab's own theme stays its own, and Settings on a tab wearing its
  group's makes that tab's change its own look (`keep_worn`), as the
  design says.
- **Its start:** a tab or pane that follows its group starts its shell in
  the group's theme (its start screen, its prompt), still following the
  group: `open_tab_in` (⌘T and the rest, from a tab in the group),
  `open_pane` (splits and Restart Tab), the flow command's tab and
  `reopen`. A tab already open takes the theme's colors, font, CRT and
  frame at once, and its prompt and start screen with its next shell,
  since the shell's prompt is set as it starts.
- **Moves:** `regroup` keeps what's worn where it belongs first
  (`keep_worn`, so the change isn't taken for Settings'), and where any
  tab now wears another theme (moved into a themed group or out, or a
  group gone), the window wears afresh, every tab is laid out to its own
  theme's grid and its programs answered in its colors
  (`follow_group_themes`).
- **The menu:** a group's has **Theme…** after its colors
  (`MenuItem::Theme`), which opens its themes in the menu's place
  (`tab_sidebar::theme_menu`, `MenuItem::GroupTheme`): Settings' theme,
  then each of `looks::ids`, the one it wears ticked and the keys on it.
  Choosing one is `App::set_group_theme`, with a toast saying so. A menu
  longer than the window shows what fits around the item the keys are
  on, with a bar (`ui::draw_row_menu`).
- **Kept:** `OpenGroup::theme`, left out where none; read back only where
  that theme is still there (`kept_group`). Deleting a saved theme takes
  it off the groups that wore it too.

### Names

A tab or a group is named in place, its name typed as Settings' text rows
are (`settings::TextField`). You can start naming with a double click on
its name, in the sidebar or the title bar, with F2 while the sidebar has
the keys, or with Rename in its menu.

- Enter keeps the name and Esc puts back the old one.
- An empty name gives a tab its shell's name back.
- A tab's name stands in for `tab_names`' one everywhere a tab is named:
  the title bar, the card under a tab, screen readers, the palette and
  Settings on a tab.

As built in step 1 (2026-11-28, `core/src/app/tab_names.rs`):

- **Numbers kept** (the user, 2026-11-28, trying step 3: a tab closing
  renumbered the rest, so which had gone wasn't clear): each tab keeps the
  number it opened with (`Tab::number`, the lowest its shell's open tabs
  don't have, `free_number`, kept in `tabs.json` as `OpenTab::number`), and
  `tab_names` shows it unless it's 1 and no other unnamed tab runs that
  shell.

- **Starting:** a double click on a tab names it, the first click having
  brought it forward. Rename Tab (`Command::RenameTab`, `rename_tab` in
  `keys::TABLE`, no key at first) does it from the palette and the Mac's
  Shell menu.
- **The field** is the tab's own pill (`ui::draw_tab_field`), as wide as
  the name and room for a few letters more. It holds the tab's name, or
  the one it's called without one, without the theme the title bar adds
  after it.
- **The keys:** every key goes to it while it's there
  (`App::rename_key`), as Settings' text rows take them, and so do
  pastes and the input method's text. Enter and Tab keep the name, and
  so does a click anywhere but the pill, or a command from the Mac's
  menus other than Paste.
- **Kept:** a name is held to one line of at most `TAB_NAME_MOST` (60)
  letters, with no control characters and no blanks at its ends
  (`tab_name`), as typed and as read from `tabs.json` (`OpenTab::name`).
  A name typed as the one the tab is called anyway is none, so it follows
  its shell again, numbered as the others come and go.
- **Shown:** a named tab shows its name as typed, with no ` · <theme>`
  after it. The other tabs that run the same shell are numbered among
  themselves alone.
- **Readers** get it as a dialog of its own (`DialogKind::TabName`), its
  field with the keys, and F1 opens Flow's own page, which says how.

### In `tabs.json`

- **`OpenTabs`** gets `groups: Vec<OpenGroup>`: each group's name, color,
  folded and theme, in the sidebar's order.
- **Each `OpenTab`** gets `group: Option<usize>` (its group's place in
  `groups`) and `name`.
- **Both are left out where empty,** so a file with no groups is as
  before.
- **Reading it back,** a group no tab names is passed over, and a tab
  naming a group past the list's end is loose.
- **An older Flow** reopens the tabs loose, without their names, and
  writes the file back without them.
- **A guest Flow** (`flow --window`) keeps no `tabs.json`, so its groups
  go with it.

## The sidebar

A card of glass down the window's left side, from under the title bar to
the window's foot, drawn by `ui::draw_sidebar` over the terminal as the
palette and the panel are drawn. Its rows, top to bottom:

- **Loose tabs, groups and their tabs, in `App::tabs`' order:**
  - A group's header has its color mark, its name and how many tabs it
    holds. Under the pointer it shows a × that closes the group, and a …
    that opens its menu.
  - Under each header come its tabs, indented, unless the group is
    folded.
- **A tab's row** has its name and, under it, its line (below), and the
  alert's dot where it glows.
  - The tab in front is lit as its pill in the title bar is.
  - Under the pointer the row shows a × and a … for its menu.
  - Resting there shows the card under a tab beside it, as in the title
    bar.
- **A split tab** folds open to show its panes, a row each, with each
  pane's line. A click on a pane brings it in front in its tab. It's
  folded at first, and the fold isn't kept.
- **At the foot,** New tab (the default shell, as the + does), New group,
  and the pin.

Where the rows don't fit, the list scrolls, as Settings' does, keeping
the row the keys are on in view. The wheel scrolls it.

As built in step 3 (2026-11-28, `core/src/tab_sidebar.rs`,
`core/src/app/sidebar.rs`, `ui::draw_sidebar`):

- **The pure part** is `tab_sidebar`: `rows` (from `Listed` tabs, the
  groups and the split tabs folded open), `line` (from a pane's `Doing`),
  `folder_shown`, `arrow` (what → and ← do), `starting_with` (a letter),
  the menus (`tab_menu`, `group_menu`, `MenuItem`, `words`) and the state
  (`Sidebar`: out since when, the chosen row by its `RowId`, the first row
  shown, the split tabs folded open, a menu, Close Group's first press,
  which `group_closes_now` weighs). A row is a `RowId` (a group, a tab or a pane by
  ids), so the keys stay on it as tabs move; where it goes, `keep_chosen`
  gives the keys to its group's header or the row where it was.
- **The line** reads the pane's `ShellState`: `progress`, `command` while
  `running_since` holds and it isn't at its prompt, `ended` (new: the last
  command that ended with a code, kept until the next ends, never taken)
  and `cwd`, its home the snapshot's own in snapshots
  (`shell::plain_home`).
- **Drawing:** `ui::SidebarView` is made afresh each frame while it's out
  (`App::sidebar_view`, owned, so it's made before the renderer is
  borrowed), and `Drawn::sidebar_top` hands back the first row the drawing
  kept the chosen one in view by. Group headers are `1.8` lines tall,
  tabs and panes `2.9` (name and line). It's drawn after the terminal and
  the panel, and clears the hits under it, as the palette does; the tab
  card shows beside a row (`Hit::SidebarRow`, mapped to its tab in
  `on_mouse_move`). Its menu is drawn under the row's …, over it where
  there's no room below.
- **A click on a tab's row** brings the tab forward, the sidebar staying
  out, and a second click makes a double click, which names it. A double
  click on a group's header puts its fold back and names it.
- **Renaming** (`tab_names::Named`, a tab or a group) types in the row
  while the sidebar is out and in the pill while it's in; a group's name
  left empty keeps the one it had. New Group from the sidebar's foot, its
  menu's Move to New Group, and New Group anywhere while it's out, name the
  group at once.
- **Keys** go to `App::sidebar_key` after Settings and the new-tab picker,
  before the find bar: its own, a menu's while one is open, the context-menu key
  too, and every other key as it's bound (`do_bound`);
  nothing reaches the terminal. Opening the palette, Settings, the history
  or About puts it away.
- **Moving with the keys:** `TabGroups::step_tab` and `step_group` (a tab
  passes its neighbour where they share a group or are both loose, leaves
  its group at the group's edge, joins a group it's loose beside; a group
  passes a loose tab or a whole group). A tab moved into a folded group
  opens it.
- **The width** is `tab_sidebar::WIDTH`, 260 points, at most a third of
  the window, until step 4 makes it a setting.
- **Readers** get a `DialogKind::TabSidebar`: its rows a list of
  `Chooser::Sidebar`, drawn as a `Tree` of `TreeItem`s with their level
  and, for a group or a split tab, expanded or collapsed and the action
  that turns it over (`Do::Fold`), a menu as `Chooser::SidebarMenu`'s
  `Menu` and `MenuItem`s, and the foot's buttons; the title bar's button
  is a toggle in `lead`. `App::sidebar_focus` names the row or menu item
  the keys are on, and a name being typed is the `TabName` field over it
  (`App::naming_dialog`), titled for a group where it's one.
- **Snapshots** have a stage of their own after the scrolled pane
  (`SIDEBAR_STAGE`): four tabs opened in the snapshot's shell, given two
  and a half seconds to settle, then made up (`App::show_sidebar`) as Web
  (`server` running `npm run dev`, `tests` on the branch `main`) and
  Docs, folded, beside the split tab folded open to its panes, the count's
  `false` its ✗; captured as `tab-sidebar.png`, then closed again.

### Over the terminal, or pinned

- **It opens and closes** with its key, ⇧⌘S on the Mac and Ctrl+Shift+S
  elsewhere (`Command::TabSidebar`, `tab_sidebar` in `keys::TABLE`, which
  Keys can change). It also opens from:
  - the title bar's button, three lines drawn left of the first tab
    (`Hit::TabSidebar`);
  - the palette;
  - the Mac's View menu.
- **Popped out,** it lies over the terminal's left part and doesn't
  change the terminal's size, so no shell sees a resize.
  - It slides out over `SIDEBAR_SLIDE` (about 150 ms), at once in
    efficient mode, and draws no frames once it's still.
  - It takes the keys, as the palette does.
  - Choosing a tab brings that tab forward and leaves the sidebar out
    (the user, 2026-11-28, trying step 3: it was to close it, but then
    looking through tabs took it out again for each); a click outside it,
    Esc and its key again put it away.
- **Pinned** (`ui.tab_sidebar_pinned`, the pin, Settings' Window section,
  the View menu's Pin Tab Sidebar), it stays out, and the terminal
  narrows to make room for it.
  - The keys go back to the pane in front, and its key moves them in and
    out of the sidebar.
  - Choosing a tab brings it forward and gives it the keys.
  - The pin again unpins it, leaving it out over the terminal, popped,
    with the keys, until Esc or a click outside it puts it away (the
    user, 2026-11-28, trying step 4: it put it away at once).
- **Its width** is `ui.tab_sidebar_width` points (260 at first), changed
  by dragging its right edge as a divider is dragged, and kept as
  Settings keeps a change.
  - It's at most a third of the window.
  - Pinned in a window too narrow to leave the terminal 20 columns, it
    pops out over the terminal instead, until the window is wider. The
    config still says pinned.

As built in step 4 (2026-11-28):

- **The settings** are `window.tab_sidebar` (`tab_sidebar::POPS_OUT`,
  `PINNED`) and `window.tab_sidebar_width` (points, `TAB_SIDEBAR_WIDTH`'s
  160 to 600), in the Window part of Settings beside Always on top and
  `window.on_top`, not `ui.*` as first written. Both are open to
  `flow-settings`.
- **The pin** is a pushpin at the foot, after New Tab and New Group
  (`Hit::SidebarPin`, `App::toggle_sidebar_pin`, saved at once);
  `App::follow_sidebar_pin` brings it out pinned at the start, as Settings
  or a program changes it, and as the window grows back past room for it
  (`tab_sidebar::pin_fits`, `PIN_LEAST_COLS`), and puts it away where
  Settings or a program unpins it (`Sidebar::pinned`); its own pin leaves
  it out, popped (2026-11-28).
- **The pin's look** (the user, 2026-11-28): a pushpin leaning over to
  the right while it isn't pinned, and upright, pressed in to its collar
  on a fainter line for the board, in the accent, while it is
  (`ui::draw_pin`). Its head, body, collar and needle are segments with
  round ends (`Canvas::capsule`, the stroke kind's `params.y` below -1.5,
  whose distance in `ui.wgsl` is to the segment rather than its line), so
  it turns any way.
- **The keys:** `Sidebar::keys`, true while it's popped out; pinned, its
  key, a click on it (not a tab's or a pane's row) or a menu give it them,
  and choosing a tab or a pane, Esc, a click elsewhere and anything that
  takes them (the palette, Settings, the history, About:
  `App::sidebar_aside`) give them back. `sidebar_has_keys` is what `on_key`
  and help go by, and the row the keys are on is outlined only then.
- **The pointer:** pinned, the sidebar takes only its own hits; the
  terminal keeps the rest (`covered` is false), and a press there gives
  the keys back to the pane.
- **The layout:** `ui::Around` (the panel, a border, and `left`, the
  pinned sidebar's room in pixels: its margin and width,
  `App::sidebar_room`) is what `Layout::compute`, `size_for_part` and
  `text_size_within_part` take, so the grid, `grid_wearing`, Fit window to
  and Fit text to all count it. `record_crop` is the terminal's area:
  below the title bar, above the panel (it read the panel as at the
  right, which it hasn't been) and right of a pinned sidebar.
- **The width's drag:** `Hit::SidebarEdge`, a strip along its right edge,
  sets `window.tab_sidebar_width` as it moves (`drag_sidebar_edge`, the tab
  in front's shells following at once) and lays every tab out and saves as
  it's let go (`sidebar_edge_let_go`).
- **Snapshots** show it pinned beside the split tab as
  `tab-sidebar-pinned.png`, after `tab-sidebar.png`.

### The layout, pinned

`Layout::compute` takes the sidebar's room on the left, `left`, in
points. It lays out the rest as a window that much narrower would be,
moved right by it: the title bar stays the window's whole width, and the
terminal, a border's margins, a theme's frame and the panel all sit right
of the sidebar. Everything that asks the layout what a grid needs takes
the same room:

- `size_for_part` and `text_size_within_part`, so Fit window to and Fit
  text to count the sidebar;
- `grid_wearing` and `own_grid`, so tabs behind are sized for it;
- `record_crop`, so a recording leaves out a pinned sidebar (one popped
  out over the terminal is in the picture, as the palette would be).

Pinning or unpinning resizes every tab's shells once, as the panel
opening does.

### Keys while it has them

| Key | Does |
|---|---|
| ↑ ↓ | Moves through the rows |
| Enter | Brings the tab or pane forward, or folds a group open or shut |
| → ← | Folds a group or a split tab open or shut, or goes to the group's header |
| F2 | Names the row's tab or group |
| ⌥↑ ⌥↓ (Alt+↑, Alt+↓) | Moves the tab up or down, into the next group or out of its own at a group's edge, or moves a group |
| ⌘⌫ (Ctrl+Delete) | Closes the row's tab, or its group, as their × does |
| Esc | Closes the sidebar, or, pinned, gives the keys back to the pane |
| F1 | Its help page |

Typing a letter goes to the first row whose name starts with it, as a
list does. Every other shortcut of Flow's still works.

### Menus

A tab's … holds:
- Rename;
- Move to Group, which lists each group and New Group;
- Ungroup, for a tab in a group;
- Restart Tab;
- Close.

A group's … holds:
- Rename;
- Color, the eight;
- Theme, the themes list;
- Ungroup;
- Close Group.

Both menus are drawn by Flow, as the new-tab picker is, so they are the
same on every OS and readers get them as menus. The palette also gets
these, built as it opens, so all of it can be done without a pointer:
- Go to Tab: `<name>`, one for each tab;
- Move Tab to Group: `<name>`, one for each group;
- New Group;
- Tab Sidebar.

### Several tabs at once, and a tab's menu in the title bar

Asked for on 2026-11-28, trying step 3: tabs picked together in the
sidebar to make a group of, and a right click on a tab in the title bar
that can make one.

- **Picking:** in the sidebar, ⌘-click (Ctrl+click elsewhere) adds a
  tab's row to what's picked or takes it out, and Shift-click picks every
  tab from the row the keys are on to the one clicked; Shift+↑ and ⇧↓ do
  the same from the keyboard, from where the pick began. A plain click or
  arrow starts over, picking nothing. Only tabs are picked: group headers
  and panes are passed over, and a folded group's tabs aren't rows. Esc
  lets go of what's picked first, then puts the sidebar away. Picked rows
  are lit with a bar at their left.
- **With tabs picked:** New Group (the foot, the palette, its key) puts
  them all into one new group, in their order, gathered at the first
  one's place, its name ready to be typed. A picked row's … menu acts on
  all of them, saying how many: New Group, Move to each group, Remove
  from Group, and Close (asking once where any is busy, as a tab does);
  Rename and Restart Tab are for one tab and leave the menu then.
- **A tab's menu in the title bar:** a right click on a tab's pill (a
  Control-click on the Mac) opens the same menu as its row's … in the
  sidebar, drawn by Flow under the pill: Rename, Move to New Group and to
  each other group, Remove from Group, Restart Tab and Close. It takes
  the keys while it's open (↑ ↓, Enter, Esc) and a click outside closes
  it. A right click elsewhere on the title bar opens the window manager's
  menu, as before.
- **The moves** are `TabGroups`' own, on whatever tabs a test picks
  (`group_picked`, `join_picked`, `leave_picked`), so the order rules hold
  for several tabs as for one.

As built (2026-11-28): `Sidebar::picked` and `anchor` (`pick`, `pick_to`,
`unpick`, `several`: more than one, since a pick of one is the row's
own), `tab_sidebar::between` for a run, `picked_menu` and `Menu::many`
(words with counts), `Menu::pill` for the title bar's (`App::pill_menu`,
`pill_menu_open`, `pill_menu_click`, `menu_key`, drawn by
`ui::draw_row_menu` under the pill, `View::pill_menu`). Closing several
asks once for them all (`App::close_tabs_asking`, `CloseOf::Tabs`), and
always where they're every tab. Move to New Group from a pill, where no
header shows to type a name in, says where the tab went instead. Readers
get the tree as multiselectable, each picked tab selected, and a pill's
menu as a menu of its own with the keys.

### Closing a group whole

Close Group closes every tab in it at once where each waits at its
prompt (`Session::busy_with`). Where one doesn't, the first press says
which tabs are busy, in the row and a toast, and a second press within
`CONFIRM_WINDOW` closes them, as Settings' Remove asks twice. Where the
group holds every tab, closing it quits Flow, so it always asks twice and
says so.

A single tab or pane asks the same way (the user, 2026-11-28, trying step
3, a busy tab gone at once from its ×): its × in the title bar or the
sidebar, ⌘W, ⌘⌫ on its row and a pane's ✕ close it at once while nothing
runs in it (`Session::busy_with`: integrated, not at its prompt and not
drawing the next), else the first says what runs (`toast.close_running`)
and the same close again within `CONFIRM_WINDOW` closes it
(`App::close_now`, `close_tab_asking`, `close_pane_asking`, `CloseOf`).
A shell that never said anything counts as waiting, so it never asks.
The last tab closed from its row in the sidebar (its ×, its menu's
Close, ⌘⌫) quits Flow, so it asks first (`sidebar.last_tab`), as Close
Group does for every tab; before, the row's × was hidden and Close and
⌘⌫ did nothing there, with no word why (the user, 2026-11-28).

### Dragging

- **In the sidebar:**
  - A tab dragged by its row moves among the tabs, into a group (on its
    header, or between its tabs) or out of one, a line showing where it
    will land.
  - A group dragged by its header moves with all its tabs.
  - A drag starts past a few points of travel, so a click is still a
    click.
- **In the title bar:** a tab dragged by its pill moves along the bar, as
  in the sidebar. Dropped among a group's run, it joins that group; past
  the run's end, it's loose. It's new to the title bar too, and it comes
  in the same step as the sidebar's dragging.

Each drag goes through `TabGroups`' moves, so the rules for the order
are the same however a tab moves.

As built for the sidebar (2026-11-28, the user asking for a tab dragged
into a group), then the title bar (the same day):

- **Where it lands** is `tab_sidebar::drop_at`, from the row under the
  pointer and how far down it is:
  - a tab on a header's middle half goes into that group, after its tabs
    (`Drop::Into`), its group opened so it's seen there;
  - anywhere else it lands in the gap nearer the pointer (`Drop::Gap`),
    never among a split tab's panes (after them). A gap is a group's
    where the row below it is one of its tabs, so between a header and
    its first tab, or between two of its tabs, it joins that group there;
    the gap after a group's last tab is loose, which takes a tab out of
    its group;
  - a group lands only in a loose gap: over another group's rows, the
    nearer end of it;
  - none where it would go nowhere new: beside itself, as it is.
- **The moves:** `TabGroups::place` (a tab before a given tab, into a
  group or loose, refused where it would part a group's tabs) and
  `place_group` (a group's tabs together, refused inside another's),
  with `tab_sidebar::landing` for the tab a gap goes before. The test of
  any moves keeping every group's tabs together makes both among its
  moves.
- **The drag:** a press on a tab's row or a group's header
  (`Sidebar::pressed`) becomes a drag (`Sidebar::drag`) once the pointer
  has gone `DRAG_AFTER` (4 points). A tab's press brings it forward at
  once as before; a header's fold waits for the press to be let go, so
  a header can be dragged without folding it. `App::drop_under` reads the
  rows as the frame drew them, none outside the card.
- **What shows:** a line in the gap (set in and in the group's color
  where it would join one, the accent at the left where loose), or the
  header lit in its color; the row dragged dimmed where it was; its name
  in a chip beside the pointer; the grabbing hand; and no row's × or …
  or tab card meanwhile.
- **Letting go** drops it (`App::drop_sidebar_row`), the keys on the row
  moved; outside the card, after Esc, or once Flow's window loses the
  keys, it stays where it was.
- **In the title bar** (`tab_sidebar::bar_drop`, `Drop::Bar`): a press
  on a pill, which brings it forward as before, drags it past
  `DRAG_AFTER`. It lands before or after the pill under the pointer by
  the half it's in, joining that pill's group; over a loose pill, past
  the last, or in the outer quarter of a group's first or last pill, it
  lands loose. So a tab can join a group at either end, or land between
  two groups that touch, and no group is parted. `TabGroups::place`
  makes the move, as from the sidebar. A line in the gap (the group's
  color where it would join one) shows where, the pill dragged is
  dimmed, and no × shows meanwhile.
- **On the Mac,** AppKit moved the whole window as a pill was dragged
  (the user, trying it): its title strip is Flow's whole title bar
  (`place_traffic_lights`), and winit's view let a drag starting there
  move the window. winit's view class now answers
  `mouseDownCanMoveWindow` no (`macos/src/window.rs`,
  `keep_still_under_presses`), and the window moves only from the title
  bar's empty space, where Flow asks AppKit to move it itself
  (`drag_window`, as it already did). Windows and Linux draw their own
  title bars, whose pills never moved the window.
- **Between the two:** one drag state serves both (`Sidebar::pressed`,
  `drag`): `App::drop_under` takes the title bar while the pointer's in
  it, or just under it, for a tab, else the sidebar's card where it's out.
  So a pill can be dropped in a pinned sidebar (a popped-out one goes as
  a pill is pressed, as before), and a tab's row in the title bar. The
  pills' places come from `ui::Drawn::pills`, kept apart from the hits,
  which a popped-out sidebar clears (`App::pills`, `pill_under`). A group
  dragged has no place in the title bar.
- **The name beside the pointer** is drawn over everything
  (`ui::draw_drag_name`, `View::tab_drag`, `App::tab_drag_view`), below
  and right of the pointer, so the line stays in view.
- **Left out:** scrolling the list as the pointer nears its ends (the
  wheel scrolls it during a drag), and dragging several picked tabs at
  once (a plain press lets them go, as a click does).

### The title bar's marks

A tab in a group has a short bar in the group's color along its pill's
foot, so a group's run reads as one. The alert's dot stays at the pill's
left. Nothing else in the title bar changes: Ctrl+Tab, Go to Tab's digits
(⌘1 to ⌘9, Ctrl+Shift+1 to 9 elsewhere) and the tab card go through
every tab in `App::tabs`' order, folded groups too.

## The line under each tab

In order, it says the first of these the tab has:

1. a program's progress, as a thin bar with its percentage;
2. the command running, cut to the row's width, with a steady dot;
3. how the last one ended, ✓ or ✗ with its command, until the next
   starts;
4. its folder, `~` for home and cut from the left.

An alert's glow lights the row as it lights the pill. The assistant's
pane and an outside app's are named in their pane's row.

None of these keep frames coming: the row is drawn as each frame is,
and output already brings frames. There's no running clock, since a clock
would need a frame each second.

**A git branch** comes in its own step: where the folder is in a git
checkout, the line's folder gives way to the branch. A `flow-git` thread
looks up from the folder for `.git` (a folder, or a worktree's file with
its `gitdir:`), reads `HEAD`, and hands the branch back as a `UserEvent`.
It reads only when the folder changes or a command ends, keeping each
folder's answer until then, and never on the event loop. It runs no
`git`.

As built in step 7 (2026-11-28):

- **Reading** is `core/src/git_branch.rs`: `head_of` looks up from the
  folder for `.git` (a folder, or a file whose `gitdir:` names one, from
  where it is where it's relative, as a submodule's is) and reads its
  `HEAD` (`parse_head`): `ref: refs/heads/<branch>` is a branch, a 40 or
  64 letter id a detached commit (`Head::Detached`, its first seven). A
  name's control characters go and it's cut at 200 letters. Only regular
  files are read, their first 4 KiB, and the folder and a `gitdir:` only
  where `link::on_this_computer` says, as every folder a shell says is
  (`untrusted-output.md`). Git Bash says its folder as `/c/…`, which
  Windows' `on_this_computer` refuses, so the folder is read as Windows
  writes it first (`tools::windows_path`; found building it on Windows,
  2026-11-28, where Git Bash's tabs showed no branch).
- **When:** `git_branch::Branches` keeps each folder's answer and those
  being read. While the sidebar is out, `App::ask_branches` reads the
  folders of every pane not yet read on a `flow-git` thread, each coming
  back as `UserEvent::GitHead`; a command ending in a pane forgets its
  folder (`watch_commands`), so the next look reads it again. Snapshots
  read none: their branch is made up.
- **The line:** `Doing::branch`, `Line::Branch` in the folder's place
  (after progress, the command running and the last end), drawn after a
  branch's mark (a commit's where it's detached) from round-ended
  segments, as the pin is (`ui::draw_branch_mark`), so it's the same in
  every font. Readers hear "On main" (`sidebar.on_branch`,
  `sidebar.detached`).
- **The pictures:** `tests` is on `main` now, in place of its passed
  `cargo test`, whose ✓ the split tab's first pane still shows.

## Everything else a new screen takes

- **Screen readers** (`access.rs`): the sidebar is a `Tree` whose items
  are each group (`TreeItem`, expanded or collapsed) and each tab
  (`TreeItem`, selected for the tab in front), each tab's line its
  description.
  - Their Click brings the tab forward, and Expand and Collapse fold.
  - The menus are `Menu` and `MenuItem`s.
  - The title bar's button is a toggle.
  - `access_focus` names the row the keys are on.
  - The manual's [Accessibility](../manual/accessibility.md) says so.
- **Help:** a page of its own (`help::Page::TabSidebar`, `flow-help
  tabs`), which F1 opens while the sidebar has the keys, and a section on
  tabs and groups in the manual's [The terminal](../manual/terminal.md).
  [Keys](../manual/keys.md) and the help's Keys page come from the table.
- **Settings:** Window's Tab sidebar row (Pops out, Pinned), and the
  width where it's pinned. Both are open to `flow-settings`, as
  `window.on_top` is, and both are in [Settings](../manual/settings.md)
  and [Config keys](../manual/configuration.md).
- **Words:** every one in `sidebar.*`, `menu.*` and `keys.*`, in en.json
  and ja.json.
- **Snapshots:**
  - `tab-sidebar.png`: the sidebar popped out over the demo tabs, with
    made-up groups (a folded one among them), a tab running a command, a
    ✓ and a ✗, and a tab's made-up branch.
  - `tab-sidebar-pinned.png`: pinned beside a split tab.
  - Both are made on made-up inputs alone, as every snapshot is.

## Steps

Each step is a commit of its own, tested before the next:

1. **Tab names:** typed in place in the title bar, kept in `tabs.json`.
   Useful alone. Built on 2026-11-28 (see Names).
2. **Groups:** `tab_groups.rs` and its tests, `tabs.json`, the title
   bar's marks, and the palette's New Group, Move Tab to Group and Go to
   Tab. Groups can then be made and used before the sidebar exists.
   Built on 2026-11-28 (see The groups).
3. **The sidebar over the terminal:** its rows, keys, clicks, folds,
   menus, Close Group, readers, help, and `tab-sidebar.png`. Built on
   2026-11-28 (see The sidebar).
4. **Pinned:** `Layout::compute`'s `left`, the Settings row, the width's
   drag, and `tab-sidebar-pinned.png`. Built on 2026-11-28 (see Over the
   terminal, or pinned).
5. **Dragging,** in the sidebar and the title bar. Built on 2026-11-28
   (see Dragging).
6. **A group's theme.** Built on 2026-11-28 (see A group's theme).
7. **The git branch on the line.** Built on 2026-11-28 (see The line
   under each tab).

## Tests

- **`tab_groups`:** moving a tab into, out of and between groups keeps
  every group's tabs together, and a group goes with its last tab. New
  tabs land at their group's end, and moving groups keeps their tabs in
  order.
- **`tabs.json`:**
  - a file with groups and names reads back as written;
  - one without them reads as before;
  - a tab naming a group that isn't there is loose.
- **The layout:** with `left`,
  - the terminal starts right of the sidebar;
  - `size_for_part` gives a window wider by its room;
  - a window too narrow to pin falls back to popping out.
- **The sidebar's rows,** laid out by a pure function from what the tabs
  and groups are (as `settings::entries` is), folded and open, a split
  tab's panes among them.
- **The line under a tab** picks progress, then the running command, then
  the last end, then the folder.
- **The git branch** is read from `HEAD`, from a worktree's `gitdir:`,
  from a detached head (its short id), and from no checkout at all.
- **Readers:** the tree's items, their states and what their actions do,
  read back with `accesskit_consumer`.
- **Also:** `keys::TABLE`'s no-clash test with the new key, i18n, and
  `cargo test --workspace` with clippy at its warnings.

## Left out

- **A server's address on the line** (the picture's `localhost:8765`): it
  needs the ports each tab's programs listen on, which
  `Platform::processes` doesn't read. Each OS would need its own code:
  - the Mac's `proc_pidfdinfo`'s sockets;
  - Linux's `/proc/net/tcp` matched to each program's descriptors;
  - Windows' `GetExtendedTcpTable`.

  It's a design of its own once the rest is in: BACKLOG.md's A server's
  address on a tab's line (2026-11-28).
- **The count of a checkout's changed files:** it would run `git status`
  in the user's folders each time a command ends, which on a large
  checkout takes seconds. The branch alone reads one file.
- **Empty groups kept as places to open tabs into**, and a group reopened
  whole later: decided against. A group lives with its tabs.
- **A group's own folder for its new tabs:** not asked for. A new tab
  starts where the tab in front is, as now.
- **Hovering at the window's edge to open it:** it would open while
  selecting text in the first column, so it opens only from its key, its
  button, the palette and the menu.
- **Groups for the flow command and outside apps**, such as `flow
  --group web` or the `flow` MCP server's `list_tabs` saying each tab's
  group: later, once groups are in.
