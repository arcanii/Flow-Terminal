//! What an MCP server asks the user (MCP's elicitation), a link a call
//! needs opened first, and a prompt's arguments: a card over the window
//! (`ui::ServerAsk`), the first of `Servers::asks` in front, naming the
//! server at its head. A form's fields take the keys in turn (Tab, Shift+
//! Tab, ↑ and ↓), text typed into the one that has them; a link shows its
//! whole address, host lit, and is opened only on the user's word. Send
//! checks the form as its schema says, and Flow sends what's typed to the
//! server alone, never to the model itself (the server may hand it back in
//! its tool's result).

use serde_json::{Map, Value};

use super::*;
use crate::mcp::client::elicit::{self, Answer as Given, Kind};
use ui::{AskBody, ServerAsk};

/// What a card is for, which says where its answer goes.
pub(super) enum Purpose {
    /// The server's request `id`.
    Server { id: Value },
    /// A link the call of `job` needs opened first.
    Link { job: String },
    /// Prompt `prompt`'s arguments, to get it with.
    Prompt { prompt: String, given: Map<String, Value> },
}

/// The user's answer to a card.
pub(super) enum Answer {
    /// Sent (a form's content), or the link opened.
    Accept(Option<Value>),
    Decline,
    /// Put away without a choice (Esc).
    Cancel,
}

pub(super) struct AskCard {
    pub server: String,
    pub purpose: Purpose,
    pub ui: ServerAsk,
}

impl AskCard {
    /// A form asked by `server`, its fields begun from their defaults (a
    /// prompt's from the words typed after its command, `prompt`).
    pub fn form(server: &str, purpose: Purpose, form: elicit::Form, prompt: Option<(&str, &Map<String, Value>)>) -> AskCard {
        let mut answers = form.start.clone();
        if let Some((_, given)) = prompt {
            for (field, answer) in form.fields.iter().zip(answers.iter_mut()) {
                if let Some(text) = given.get(&field.key).and_then(Value::as_str) {
                    *answer = Given::Text(text.to_string());
                }
            }
        }
        let texts = answers
            .iter()
            .map(|a| match a {
                Given::Text(text) => TextField::long(text),
                _ => TextField::long(""),
            })
            .collect();
        let picks = answers.iter().map(|a| if let Given::One(Some(i)) = a { *i } else { 0 }).collect();
        let problems = vec![None; form.fields.len()];
        let message = form.message.clone();
        let ui = ServerAsk { server: server.to_string(), prompt: prompt.map(|(name, _)| name.to_string()), message, body: AskBody::Form { form, answers, texts, picks }, focus: 0, problems };
        AskCard { server: server.to_string(), purpose, ui }
    }

    /// A link `server` asks the user to open.
    pub fn link(server: &str, purpose: Purpose, link: elicit::LinkAsk) -> AskCard {
        let message = link.message.clone();
        // Its Open isn't where the keys begin: the user reads the link first.
        let ui = ServerAsk { server: server.to_string(), prompt: None, message, body: AskBody::Link(link), focus: 1, problems: Vec::new() };
        AskCard { server: server.to_string(), purpose, ui }
    }
}

impl App {
    /// The card in front.
    fn ask_card(&mut self) -> Option<&mut ServerAsk> {
        self.servers.asks.front_mut().map(|c| &mut c.ui)
    }

    /// The card's Send (or Open): a form checked first, its problems shown
    /// under their fields where it has any.
    pub(super) fn send_server_ask(&mut self) {
        let Some(card) = self.ask_card() else { return };
        let content = match &card.body {
            AskBody::Link(_) => None,
            AskBody::Form { form, answers, .. } => match form.content(answers) {
                Ok(content) => Some(content),
                Err(problems) => {
                    card.problems = vec![None; form.fields.len()];
                    for (i, why) in &problems {
                        card.problems[*i] = Some(why.clone());
                    }
                    card.focus = problems[0].0;
                    self.request_redraw();
                    return;
                }
            },
        };
        self.answer_server_ask(Answer::Accept(content));
    }

    /// A key while a server's card is in front: it takes them all.
    pub(super) fn server_ask_key(&mut self, event: &KeyEvent) {
        let mods = self.modifiers;
        let mac = cfg!(target_os = "macos");
        let (ctrl, shift) = (mods.control_key(), mods.shift_key());
        let command = ctrl || mods.super_key();
        let word = if mac { mods.alt_key() } else { ctrl };
        let jump = if mac { mods.super_key() } else { false };
        let base = match event.key_without_modifiers() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let Some(card) = self.servers.asks.front_mut().map(|c| &mut c.ui) else { return };
        let items = card.items();
        let (send, decline) = (items - 2, items - 1);
        let focus = card.focus.min(items - 1);
        let next = |card: &mut ServerAsk, by: isize| card.focus = (card.focus as isize + by).rem_euclid(items as isize) as usize;
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => return self.answer_server_ask(Answer::Cancel),
            Key::Named(NamedKey::Tab) => next(card, if shift { -1 } else { 1 }),
            Key::Named(NamedKey::Enter) if command => return self.send_server_ask(),
            Key::Named(NamedKey::Enter | NamedKey::Space) if focus == send => return self.send_server_ask(),
            Key::Named(NamedKey::Enter | NamedKey::Space) if focus == decline => return self.answer_server_ask(Answer::Decline),
            // Enter on a field goes to the next, on the last it sends.
            Key::Named(NamedKey::Enter) => {
                if focus + 1 >= send {
                    return self.send_server_ask();
                }
                next(card, 1);
            }
            Key::Named(NamedKey::ArrowUp) => next(card, -1),
            Key::Named(NamedKey::ArrowDown) => next(card, 1),
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) if focus >= send => card.focus = if focus == send { decline } else { send },
            _ => {
                let AskBody::Form { form, answers, texts, picks } = &mut card.body else { return };
                let Some(field) = form.fields.get(focus) else { return };
                let right = matches!(event.logical_key, Key::Named(NamedKey::ArrowRight));
                let left = matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft));
                let space = matches!(event.logical_key, Key::Named(NamedKey::Space));
                match (&field.kind, &mut answers[focus]) {
                    (Kind::Switch, Given::Switch(on)) if space || left || right => *on = !*on,
                    (Kind::One(options), Given::One(picked)) if left || right || space => {
                        let n = options.len();
                        let at = match (*picked, right || space) {
                            (None, true) => 0,
                            (None, false) => n - 1,
                            (Some(i), true) => (i + 1) % n,
                            (Some(i), false) => (i + n - 1) % n,
                        };
                        *picked = Some(at);
                        picks[focus] = at;
                    }
                    (Kind::Many { options, .. }, Given::Many(on)) if left || right || space => {
                        let n = options.len();
                        if space {
                            on[picks[focus]] = !on[picks[focus]];
                        } else {
                            picks[focus] = (picks[focus] + if right { 1 } else { n - 1 }) % n;
                        }
                    }
                    (Kind::Text { .. } | Kind::Number { .. }, Given::Text(text)) => {
                        let f = &mut texts[focus];
                        match &event.logical_key {
                            Key::Named(NamedKey::Backspace) if word => f.backspace_word(),
                            Key::Named(NamedKey::Backspace) => f.backspace(),
                            Key::Named(NamedKey::Delete) if word => f.delete_word(),
                            Key::Named(NamedKey::Delete) => f.delete(),
                            Key::Named(NamedKey::ArrowLeft) if jump => f.home(),
                            Key::Named(NamedKey::ArrowLeft) if word => f.word_left(),
                            Key::Named(NamedKey::ArrowLeft) => f.left(),
                            Key::Named(NamedKey::ArrowRight) if jump => f.end(),
                            Key::Named(NamedKey::ArrowRight) if word => f.word_right(),
                            Key::Named(NamedKey::ArrowRight) => f.right(),
                            Key::Named(NamedKey::Home) => f.home(),
                            Key::Named(NamedKey::End) => f.end(),
                            Key::Named(NamedKey::Insert) if shift => return self.paste(),
                            _ if command && base == "v" => return self.paste(),
                            _ if mods.super_key() || (ctrl && !mods.alt_key()) => {}
                            _ => {
                                if let Some(typed) = &event.text {
                                    f.insert(typed);
                                }
                            }
                        }
                        *text = f.text();
                    }
                    _ => {}
                }
                card.problems.iter_mut().for_each(|p| *p = None);
            }
        }
        self.request_redraw();
    }

    /// Text typed or pasted while a server's card is in front: into its
    /// field that has the keys. True where it took it.
    pub(super) fn server_ask_type(&mut self, typed: &str) -> bool {
        let Some(card) = self.ask_card() else { return false };
        let focus = card.focus;
        if let AskBody::Form { answers, texts, .. } = &mut card.body {
            if let (Some(Given::Text(text)), Some(f)) = (answers.get_mut(focus), texts.get_mut(focus)) {
                f.insert(&typed.replace(['\r', '\n'], " "));
                *text = f.text();
            }
        }
        self.request_redraw();
        true
    }

    /// A click on a server's card.
    pub(super) fn server_ask_click(&mut self, hit: Option<Hit>) {
        match hit {
            Some(Hit::AskSend) => return self.send_server_ask(),
            Some(Hit::AskDecline) => return self.answer_server_ask(Answer::Decline),
            Some(Hit::AskField(i)) | Some(Hit::AskOption(i, _)) => {
                let Some(card) = self.ask_card() else { return };
                let i = i as usize;
                card.focus = i;
                if let AskBody::Form { form, answers, picks, texts } = &mut card.body {
                    let option = if let Some(Hit::AskOption(_, k)) = hit { Some(k as usize) } else { None };
                    match (form.fields.get(i).map(|f| &f.kind), answers.get_mut(i), option) {
                        (Some(Kind::Switch), Some(Given::Switch(on)), _) => *on = !*on,
                        (Some(Kind::One(_)), Some(Given::One(picked)), Some(k)) => {
                            *picked = Some(k);
                            picks[i] = k;
                        }
                        (Some(Kind::Many { .. }), Some(Given::Many(on)), Some(k)) => {
                            if let Some(o) = on.get_mut(k) {
                                *o = !*o;
                            }
                            picks[i] = k;
                        }
                        (Some(Kind::Text { .. } | Kind::Number { .. }), _, _) => texts[i].end(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// The card in front for screen readers: its words, each field's
    /// answer, the text field that has the keys, and its switches, choices
    /// and buttons.
    pub(super) fn server_ask_dialog(&self) -> Option<access::Dialog> {
        let card = &self.servers.asks.front()?.ui;
        let mut text = vec![card.message.clone()];
        let mut buttons = Vec::new();
        let mut field = None;
        match &card.body {
            AskBody::Link(link) => {
                text.push(link.url.clone());
                text.extend(link.warning());
            }
            AskBody::Form { form, answers, texts, .. } => {
                for (i, (f, answer)) in form.fields.iter().zip(answers).enumerate() {
                    let label = if f.required { format!("{} *", f.title) } else { f.title.clone() };
                    match (&f.kind, answer) {
                        (Kind::Switch, Given::Switch(on)) => buttons.push(access::Button::toggle(label, self.rect_of(Hit::AskField(i as u8)), *on)),
                        (Kind::One(options), Given::One(picked)) => {
                            for (k, o) in options.iter().enumerate() {
                                buttons.push(access::Button::toggle(format!("{label}: {}", o.title), self.rect_of(Hit::AskOption(i as u8, k as u8)), *picked == Some(k)));
                            }
                        }
                        (Kind::Many { options, .. }, Given::Many(on)) => {
                            for (k, o) in options.iter().enumerate() {
                                buttons.push(access::Button::toggle(format!("{label}: {}", o.title), self.rect_of(Hit::AskOption(i as u8, k as u8)), on.get(k).copied().unwrap_or(false)));
                            }
                        }
                        (_, Given::Text(value)) => {
                            text.push(format!("{label}: {value}"));
                            if card.focus == i {
                                let mut f = access::Field::line(label.as_str(), f.description.as_str(), value, self.rect_of(Hit::AskField(i as u8)));
                                f.caret = (0, texts[i].caret().1);
                                field = Some(f);
                            }
                        }
                        _ => {}
                    }
                    if let Some(why) = card.problems.get(i).cloned().flatten() {
                        text.push(format!("{}: {why}", f.title));
                    }
                }
            }
        }
        let (yes, no) = card.button_words();
        buttons.push(access::Button::new(yes, self.rect_of(Hit::AskSend)));
        buttons.push(access::Button::new(no, self.rect_of(Hit::AskDecline)));
        Some(access::Dialog { kind: access::DialogKind::ServerAsk, title: card.title(), rect: self.rect_of(Hit::AskCard), text, field, buttons, ..access::Dialog::default() })
    }
}
