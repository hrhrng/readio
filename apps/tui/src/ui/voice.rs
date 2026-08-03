//! The Voice workspace.
//!
//! Model acquisition and reading configuration are separate decisions shown in
//! one surface. The left pane changes what is present on this machine; the
//! right pane writes which already-present model a scope uses. Nothing crossing
//! that divider is implicit.

use std::collections::BTreeSet;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

use crate::book::Book;
use crate::config::{Config, Scope};
use crate::i18n::{Lang, current};
use crate::library::Library;
use crate::theme::{self, theme};
use crate::voice::install;
use crate::wrap::{display_width, truncate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Models,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftScope {
    Global,
    Book,
}

#[derive(Debug, Clone)]
struct BookChoice {
    id: String,
    title: String,
}

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub engine: String,
    pub voice: String,
    pub language: String,
    pub params: String,
}

#[derive(Debug, Clone)]
pub struct Save {
    pub scope: DraftScope,
    pub book: Option<(String, String)>,
    pub draft: Draft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Install(String),
    Delete(String),
    Save,
    FollowDefault { id: String, title: String },
}

pub struct Workspace {
    pub focus: Pane,
    model_at: usize,
    field_at: usize,
    scope: DraftScope,
    books: Vec<BookChoice>,
    book_at: usize,
    draft: Draft,
    editing_params: bool,
    confirm_install: Option<String>,
    confirm_delete: Option<String>,
    space_check: Option<(String, install::Space)>,
}

impl Workspace {
    pub fn new(config: &Config, library: &Library, current_book: Option<&Book>) -> Self {
        let mut books = Vec::new();
        let mut seen = BTreeSet::new();
        if let Some(book) = current_book
            && seen.insert(book.id.clone())
        {
            books.push(BookChoice {
                id: book.id.clone(),
                title: book.title.clone(),
            });
        }
        for entry in &library.entries {
            if seen.insert(entry.id.clone()) {
                books.push(BookChoice {
                    id: entry.id.clone(),
                    title: entry.title.clone(),
                });
            }
        }
        let draft = Self::draft_for(config, DraftScope::Global, books.first());
        let names = model_names(config);
        let model_at = names
            .iter()
            .position(|name| name == &draft.engine)
            .unwrap_or(0);
        Self {
            focus: Pane::Models,
            model_at,
            field_at: 0,
            scope: DraftScope::Global,
            books,
            book_at: 0,
            draft,
            editing_params: false,
            confirm_install: None,
            confirm_delete: None,
            space_check: None,
        }
    }

    fn draft_for(config: &Config, scope: DraftScope, book: Option<&BookChoice>) -> Draft {
        let chosen = config.voice_for(match scope {
            DraftScope::Global => None,
            DraftScope::Book => book.map(|book| book.id.as_str()),
        });
        let mut language = chosen.language;
        if matches!(language.as_str(), "" | "auto") {
            language = config
                .spec(&chosen.engine)
                .and_then(|spec| spec.languages.keys().next().cloned())
                .unwrap_or_default();
        }
        let voice = if chosen.name.is_empty() {
            config
                .spec(&chosen.engine)
                .map(|spec| spec.voice.clone())
                .unwrap_or_default()
        } else {
            chosen.name
        };
        Draft {
            engine: chosen.engine,
            voice,
            language,
            params: chosen.params,
        }
    }

    pub fn save_request(&self) -> Save {
        Save {
            scope: self.scope,
            book: self
                .books
                .get(self.book_at)
                .map(|book| (book.id.clone(), book.title.clone())),
            draft: self.draft.clone(),
        }
    }

    pub fn tab(&mut self) {
        self.editing_params = false;
        self.focus = match self.focus {
            Pane::Models => Pane::Config,
            Pane::Config => Pane::Models,
        };
        self.confirm_install = None;
        self.confirm_delete = None;
        self.space_check = None;
    }

    pub fn step(&mut self, delta: isize, config: &Config) {
        self.editing_params = false;
        self.confirm_install = None;
        self.confirm_delete = None;
        self.space_check = None;
        match self.focus {
            Pane::Models => {
                let count = model_names(config).len();
                if count > 0 {
                    self.model_at =
                        (self.model_at as isize + delta).rem_euclid(count as isize) as usize;
                }
            }
            Pane::Config => {
                let count = self.field_count(config);
                self.field_at =
                    (self.field_at as isize + delta).rem_euclid(count as isize) as usize;
            }
        }
    }

    pub fn activate(&mut self, config: &Config, installing: Option<&str>) -> Action {
        match self.focus {
            Pane::Models => self.activate_model(config, installing),
            Pane::Config => self.activate_field(config),
        }
    }

    fn activate_model(&mut self, config: &Config, installing: Option<&str>) -> Action {
        self.confirm_delete = None;
        let names = model_names(config);
        let Some(name) = names.get(self.model_at).cloned() else {
            return Action::None;
        };
        let Some(spec) = config.spec(&name) else {
            return Action::None;
        };
        if spec.pip.is_empty() && spec.system.is_empty() {
            self.confirm_install = None;
            self.space_check = None;
            return Action::None;
        }
        if install::model_is_ready(&name, spec) || installing == Some(name.as_str()) {
            self.confirm_install = None;
            self.space_check = None;
            return Action::None;
        }
        if self.confirm_install.as_deref() == Some(name.as_str()) {
            if self
                .space_check
                .as_ref()
                .is_some_and(|(_, space)| !space.enough())
            {
                return Action::None;
            }
            self.confirm_install = None;
            self.space_check = None;
            Action::Install(name)
        } else {
            self.space_check = install::space(&name).map(|space| (name.clone(), space));
            self.confirm_install = Some(name);
            Action::None
        }
    }

    pub fn delete(&mut self, config: &Config, installing: Option<&str>) -> Action {
        if self.focus != Pane::Models {
            return Action::None;
        }
        self.confirm_install = None;
        self.space_check = None;
        let names = model_names(config);
        let Some(name) = names.get(self.model_at).cloned() else {
            return Action::None;
        };
        let Some(spec) = config.spec(&name) else {
            return Action::None;
        };
        if installing == Some(name.as_str()) || !install::model_is_ready(&name, spec) {
            self.confirm_delete = None;
            return Action::None;
        }
        if self.confirm_delete.as_deref() == Some(name.as_str()) {
            self.confirm_delete = None;
            Action::Delete(name)
        } else {
            self.confirm_delete = Some(name);
            Action::None
        }
    }

    fn activate_field(&mut self, config: &Config) -> Action {
        let field = self.fields(config).get(self.field_at).copied();
        match field {
            Some(Field::Scope) => {
                if self.scope == DraftScope::Global && self.books.is_empty() {
                    return Action::None;
                }
                self.scope = match self.scope {
                    DraftScope::Global => DraftScope::Book,
                    DraftScope::Book => DraftScope::Global,
                };
                self.field_at = 0;
                self.draft = Self::draft_for(config, self.scope, self.books.get(self.book_at));
                Action::None
            }
            Some(Field::Book) => {
                if !self.books.is_empty() {
                    self.book_at = (self.book_at + 1) % self.books.len();
                    self.draft = Self::draft_for(config, self.scope, self.books.get(self.book_at));
                }
                Action::None
            }
            Some(Field::Model) => {
                self.cycle_model(config);
                Action::None
            }
            Some(Field::Voice) => {
                self.cycle_voice(config);
                Action::None
            }
            Some(Field::Language) => {
                self.cycle_language(config);
                Action::None
            }
            Some(Field::Params) => {
                if params_editable(config, &self.draft.engine) {
                    self.editing_params = !self.editing_params;
                }
                Action::None
            }
            Some(Field::Save) => Action::Save,
            Some(Field::Follow) => self
                .books
                .get(self.book_at)
                .map(|book| Action::FollowDefault {
                    id: book.id.clone(),
                    title: book.title.clone(),
                })
                .unwrap_or(Action::None),
            None => Action::None,
        }
    }

    fn cycle_model(&mut self, config: &Config) {
        let ready: Vec<String> = model_names(config)
            .into_iter()
            .filter(|name| {
                config
                    .spec(name)
                    .is_some_and(|spec| install::model_is_ready(name, spec))
            })
            .collect();
        if ready.is_empty() {
            return;
        }
        let next = ready
            .iter()
            .position(|name| name == &self.draft.engine)
            .map(|at| (at + 1) % ready.len())
            .unwrap_or(0);
        self.draft.engine = ready[next].clone();
        let Some(spec) = config.spec(&self.draft.engine) else {
            return;
        };
        self.draft.voice = spec.voice.clone();
        self.draft.language = spec.languages.keys().next().cloned().unwrap_or_default();
        self.draft.params.clear();
    }

    fn cycle_voice(&mut self, config: &Config) {
        let Some(spec) = config.spec(&self.draft.engine) else {
            return;
        };
        let mut voices = Vec::new();
        for voice in std::iter::once(&spec.voice)
            .chain(spec.languages.values().map(|language| &language.voice))
        {
            if !voice.is_empty() && !voices.contains(voice) {
                voices.push(voice.clone());
            }
        }
        if voices.is_empty() {
            return;
        }
        let next = voices
            .iter()
            .position(|voice| voice == &self.draft.voice)
            .map(|at| (at + 1) % voices.len())
            .unwrap_or(0);
        self.draft.voice = voices[next].clone();
    }

    fn cycle_language(&mut self, config: &Config) {
        let Some(spec) = config.spec(&self.draft.engine) else {
            return;
        };
        let languages: Vec<String> = spec.languages.keys().cloned().collect();
        if languages.is_empty() {
            self.draft.language.clear();
            return;
        }
        let next = languages
            .iter()
            .position(|language| language == &self.draft.language)
            .map(|at| (at + 1) % languages.len())
            .unwrap_or(0);
        self.draft.language = languages[next].clone();
    }

    pub fn type_param(&mut self, ch: char) -> bool {
        if !self.editing_params || ch.is_control() {
            return false;
        }
        self.draft.params.push(ch);
        true
    }

    pub fn backspace(&mut self) -> bool {
        if !self.editing_params {
            return false;
        }
        self.draft.params.pop();
        true
    }

    pub fn escape(&mut self) -> Action {
        if self.editing_params {
            self.editing_params = false;
            Action::None
        } else if self.confirm_install.take().is_some() {
            self.space_check = None;
            Action::None
        } else if self.confirm_delete.take().is_some() {
            Action::None
        } else {
            Action::Close
        }
    }

    pub fn reload(&mut self, config: &Config) {
        self.draft = Self::draft_for(config, self.scope, self.books.get(self.book_at));
    }

    fn fields(&self, config: &Config) -> Vec<Field> {
        let mut fields = vec![Field::Scope];
        if self.scope == DraftScope::Book {
            fields.push(Field::Book);
        }
        fields.extend([
            Field::Model,
            Field::Voice,
            Field::Language,
            Field::Params,
            Field::Save,
        ]);
        if self.scope == DraftScope::Book
            && self
                .books
                .get(self.book_at)
                .is_some_and(|book| config.book_voice(&book.id).is_some())
        {
            fields.push(Field::Follow);
        }
        fields
    }

    fn field_count(&self, config: &Config) -> usize {
        self.fields(config).len().max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Scope,
    Book,
    Model,
    Voice,
    Language,
    Params,
    Save,
    Follow,
}

fn model_names(config: &Config) -> Vec<String> {
    let mut names = config.engine_names();
    names.sort_by_key(|name| match name.as_str() {
        "moss" => (0, String::new()),
        "kokoro" => (1, String::new()),
        _ => (2, name.clone()),
    });
    names
}

pub fn render(
    area: Rect,
    buf: &mut Buffer,
    workspace: &Workspace,
    config: &Config,
    installing: Option<&str>,
) {
    if area.width < 20 || area.height < 8 {
        return;
    }
    let th = theme();
    let frame = Block::default()
        .title(format!(" {} ", tr("Voice 工作台", "Voice workspace")))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.border_focus));
    let inner = frame.inner(area);
    frame.render(area, buf);

    let [summary, main, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(inner);
    render_summary(summary, buf, workspace, config);

    if area.width >= 96 {
        let [models, form] =
            Layout::horizontal([Constraint::Percentage(43), Constraint::Percentage(57)])
                .areas(main);
        render_models(models, buf, workspace, config, installing, true);
        render_form(form, buf, workspace, config, true);
    } else {
        let [tabs_area, pane] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(main);
        let tabs = match workspace.focus {
            Pane::Models => format!(
                "  [{}]   {}",
                tr("模型库", "Models"),
                tr("Voice 配置", "Voice configuration")
            ),
            Pane::Config => format!(
                "  {}   [{}]",
                tr("模型库", "Models"),
                tr("Voice 配置", "Voice configuration")
            ),
        };
        Paragraph::new(tabs)
            .style(Style::default().fg(th.text_secondary))
            .render(tabs_area, buf);
        match workspace.focus {
            Pane::Models => render_models(pane, buf, workspace, config, installing, false),
            Pane::Config => render_form(pane, buf, workspace, config, false),
        }
    }

    Paragraph::new(format!(
        "  {}",
        tr(
            "tab/←→ 切换区域 · ↑↓ 选择 · ⏎ 修改/确认 · esc 关闭",
            "tab/←→ switches pane · ↑↓ chooses · ⏎ edits/confirms · esc closes"
        )
    ))
    .style(Style::default().fg(th.text_faint))
    .render(footer, buf);
}

fn render_summary(area: Rect, buf: &mut Buffer, workspace: &Workspace, config: &Config) {
    let ready = config
        .engine_names()
        .into_iter()
        .filter(|name| {
            config
                .spec(name)
                .is_some_and(|spec| install::model_is_ready(name, spec))
        })
        .count();
    let global = if config.voice.engine.is_empty() {
        tr("未配置", "not configured").to_string()
    } else {
        config.voice.engine.clone()
    };
    let summary = format!(
        "  {} · {}: {} · {}",
        tr(
            &format!("{ready} 个模型可用"),
            &format!("{ready} models ready")
        ),
        tr("全局", "global"),
        global,
        tr(
            &format!("{} 本书有独立配置", config.voice.books.len()),
            &format!("{} book overrides", config.voice.books.len())
        )
    );
    let focus = match workspace.focus {
        Pane::Models => tr(
            "模型库负责下载；不会修改右侧配置",
            "Models downloads only; it never changes configuration",
        ),
        Pane::Config => tr(
            "配置只使用已经可用的模型",
            "Configuration uses ready models only",
        ),
    };
    Paragraph::new(vec![Line::raw(summary), Line::raw(format!("  {focus}"))])
        .style(Style::default().fg(theme().text_secondary))
        .render(area, buf);
}

fn render_models(
    area: Rect,
    buf: &mut Buffer,
    workspace: &Workspace,
    config: &Config,
    installing: Option<&str>,
    divider: bool,
) {
    let th = theme();
    if divider && area.width > 0 {
        for y in area.y..area.bottom() {
            let cell = &mut buf[(area.right() - 1, y)];
            cell.set_symbol("│").set_fg(th.border);
        }
    }
    let width = area.width.saturating_sub(4) as usize;
    let mut lines = vec![Line::styled(
        format!("  {}", tr("模型库 · 下载", "Models · downloads")),
        Style::default()
            .fg(if workspace.focus == Pane::Models {
                th.accent_agent
            } else {
                th.text_secondary
            })
            .add_modifier(Modifier::BOLD),
    )];
    let names = model_names(config);
    let list_room = area.height.saturating_sub(7) as usize;
    let shown = list_room.max(2).min(names.len());
    let first = workspace
        .model_at
        .saturating_sub(shown.saturating_sub(1))
        .min(names.len().saturating_sub(shown));
    for (offset, name) in names.iter().skip(first).take(shown).enumerate() {
        let at = first + offset;
        let selected = workspace.focus == Pane::Models && at == workspace.model_at;
        let spec = config.spec(name).expect("engine name came from config");
        let ready = install::model_is_ready(name, spec);
        let state = if installing == Some(name.as_str()) {
            format!(
                "{} {}",
                theme::spinner_frame(0),
                tr("下载中", "downloading")
            )
        } else if ready {
            format!("{} {}", theme::CHECK, tr("可用", "ready"))
        } else if spec.pip.is_empty() && spec.system.is_empty() {
            format!("— {}", tr("外部服务", "external"))
        } else {
            format!("↓ {}", tr("未下载", "not downloaded"))
        };
        let marker = if selected { theme::ARROW } else { " " };
        let style = if selected {
            Style::default()
                .fg(th.text_primary)
                .bg(th.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(if ready {
                th.text_primary
            } else {
                th.text_muted
            })
        };
        let state_width = display_width(&state);
        let name_width = width.saturating_sub(state_width + 4);
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), style.fg(th.accent_agent)),
            Span::styled(
                format!("{:<name_width$}", truncate(name, name_width)),
                style,
            ),
            Span::styled(state, style),
        ]));
    }
    lines.push(Line::raw(""));
    if let Some(name) = names.get(workspace.model_at)
        && let Some(spec) = config.spec(name)
    {
        let ready = install::model_is_ready(name, spec);
        let detail = truncate(&spec.about, width.saturating_sub(2));
        lines.push(Line::styled(
            format!("  {detail}"),
            Style::default().fg(th.text_secondary),
        ));
        if let Some(profile) = install::profile(name) {
            lines.push(Line::styled(
                format!(
                    "  {} {}{}",
                    tr("规模", "Scale"),
                    tr(profile.scale_zh, profile.scale_en),
                    if profile.download_bytes == 0 {
                        String::new()
                    } else {
                        format!(
                            " · {} {}",
                            tr("下载约", "download about"),
                            install::bytes(profile.download_bytes)
                        )
                    }
                ),
                Style::default().fg(th.text_secondary),
            ));
            lines.push(Line::styled(
                format!(
                    "  {} {}",
                    tr("推荐语言", "Recommended language"),
                    tr(profile.language_zh, profile.language_en)
                ),
                Style::default().fg(th.accent_model),
            ));
        } else {
            lines.push(Line::styled(
                format!(
                    "  {} · {}",
                    tr("规模 未声明", "Scale not declared"),
                    tr("推荐语言 按配置", "Recommended language: configured")
                ),
                Style::default().fg(th.text_faint),
            ));
        }
        if workspace.confirm_install.as_deref() == Some(name.as_str()) {
            if let Some((_, space)) = workspace
                .space_check
                .as_ref()
                .filter(|(checked, _)| checked == name)
            {
                let estimate = if space.download_bytes == 0 {
                    tr("大小未知", "size unknown").to_string()
                } else {
                    format!(
                        "{} {}",
                        tr("下载约", "download about"),
                        install::bytes(space.download_bytes)
                    )
                };
                let disk = format!(
                    "{} {} {}",
                    tr("可用", "available"),
                    install::bytes(space.available_bytes),
                    if space.enough() {
                        theme::CHECK
                    } else {
                        theme::CROSS
                    }
                );
                lines.push(Line::styled(
                    format!("  {estimate} · {disk}"),
                    Style::default().fg(if space.enough() {
                        th.accent_success
                    } else {
                        th.accent_error
                    }),
                ));
            }
            lines.push(Line::styled(
                format!(
                    "  {}",
                    if workspace
                        .space_check
                        .as_ref()
                        .is_some_and(|(_, space)| !space.enough())
                    {
                        tr(
                            "空间不足，下载不会开始。",
                            "Not enough disk space; download will not start.",
                        )
                    } else {
                        tr(
                            "再次按 ⏎ 确认下载；只增加本地模型",
                            "Press ⏎ again to download; configuration will not change",
                        )
                    }
                ),
                Style::default().fg(
                    if workspace
                        .space_check
                        .as_ref()
                        .is_some_and(|(_, space)| !space.enough())
                    {
                        th.accent_error
                    } else {
                        th.accent_warning
                    },
                ),
            ));
        } else if workspace.confirm_delete.as_deref() == Some(name.as_str()) {
            lines.push(Line::styled(
                format!(
                    "  {}",
                    tr(
                        "再次按 d 删除本地模型文件；配置保持不变",
                        "Press d again to delete local model files; configuration will not change"
                    )
                ),
                Style::default().fg(th.accent_warning),
            ));
        } else if ready {
            lines.push(Line::styled(
                format!(
                    "  {}",
                    tr(
                        "连续按两次 d 删除本地模型文件",
                        "Press d twice to delete local model files"
                    )
                ),
                Style::default().fg(th.text_faint),
            ));
        } else if spec.pip.is_empty() && spec.system.is_empty() {
            lines.push(Line::styled(
                format!(
                    "  {}",
                    tr(
                        "外部服务不由 readio 下载；配置好服务后再在右侧选择",
                        "External services are not downloaded; configure one, then select it on the right"
                    )
                ),
                Style::default().fg(th.text_faint),
            ));
        } else {
            lines.push(Line::styled(
                format!(
                    "  {}",
                    tr(
                        "⏎ 查看/确认下载 · 下载后仍需在右侧保存",
                        "⏎ details/download · save on the right to use it"
                    )
                ),
                Style::default().fg(th.text_faint),
            ));
        }
    }
    Paragraph::new(lines).render(area, buf);
}

fn render_form(area: Rect, buf: &mut Buffer, workspace: &Workspace, config: &Config, inset: bool) {
    let th = theme();
    let area = if inset {
        Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(1),
            area.height,
        )
    } else {
        area
    };
    let mut lines = vec![Line::styled(
        format!("  {}", tr("Voice 配置", "Voice configuration")),
        Style::default()
            .fg(if workspace.focus == Pane::Config {
                th.accent_agent
            } else {
                th.text_secondary
            })
            .add_modifier(Modifier::BOLD),
    )];
    let fields = workspace.fields(config);
    for (index, field) in fields.iter().enumerate() {
        let selected = workspace.focus == Pane::Config && index == workspace.field_at;
        let (label, value) = field_value(*field, workspace, config);
        let marker = if selected { theme::ARROW } else { " " };
        let style = if selected {
            Style::default()
                .fg(th.text_primary)
                .bg(th.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.text_secondary)
        };
        let value_style = if matches!(field, Field::Save) {
            style.fg(th.accent_success)
        } else if matches!(field, Field::Follow) {
            style.fg(th.accent_warning)
        } else {
            style
        };
        let label_width = 12usize;
        let label_padding = label_width.saturating_sub(display_width(&label));
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), style.fg(th.accent_agent)),
            Span::styled(format!("{label}{}  ", " ".repeat(label_padding)), style),
            Span::styled(value, value_style),
        ]));
    }
    lines.push(Line::raw(""));
    if workspace.draft.engine.is_empty() {
        lines.push(Line::styled(
            format!(
                "  {}",
                tr(
                    "先在左侧下载模型，再回到这里选择。",
                    "Download a model on the left, then choose it here."
                )
            ),
            Style::default().fg(th.text_muted),
        ));
    } else if !config
        .spec(&workspace.draft.engine)
        .is_some_and(|spec| install::model_is_ready(&workspace.draft.engine, spec))
    {
        lines.push(Line::styled(
            format!(
                "  {}",
                tr(
                    "当前草稿引用的模型不可用；保存前请选择已下载模型。",
                    "The draft model is unavailable; choose a ready model before saving."
                )
            ),
            Style::default().fg(th.accent_warning),
        ));
    } else if workspace.editing_params {
        let hint = params_hint(&workspace.draft.engine);
        lines.push(Line::styled(
            format!(
                "  {}{}",
                hint,
                tr(
                    " · backspace 删除，esc 结束编辑。",
                    " · backspace deletes, esc finishes editing."
                )
            ),
            Style::default().fg(th.accent_agent),
        ));
    } else {
        lines.push(Line::styled(
            format!(
                "  {}",
                tr(
                    "这里只编辑草稿；选择“保存配置”后才生效。",
                    "This is a draft; nothing changes until Save configuration."
                )
            ),
            Style::default().fg(th.text_faint),
        ));
    }
    Paragraph::new(lines).render(area, buf);
}

fn field_value(field: Field, workspace: &Workspace, config: &Config) -> (String, String) {
    match field {
        Field::Scope => (
            tr("作用范围", "Scope").to_string(),
            match workspace.scope {
                DraftScope::Global => tr("● 全局  ○ 单本书", "● Global  ○ One book"),
                DraftScope::Book => tr("○ 全局  ● 单本书", "○ Global  ● One book"),
            }
            .to_string(),
        ),
        Field::Book => (
            tr("书籍", "Book").to_string(),
            workspace
                .books
                .get(workspace.book_at)
                .map(|book| book.title.clone())
                .unwrap_or_else(|| tr("书库为空", "Library is empty").to_string()),
        ),
        Field::Model => (
            tr("模型", "Model").to_string(),
            if workspace.draft.engine.is_empty() {
                tr("未选择", "Not selected").to_string()
            } else {
                let ready = config
                    .spec(&workspace.draft.engine)
                    .is_some_and(|spec| install::model_is_ready(&workspace.draft.engine, spec));
                format!(
                    "{}{}",
                    workspace.draft.engine,
                    if ready {
                        ""
                    } else {
                        tr(" · 不可用", " · unavailable")
                    }
                )
            },
        ),
        Field::Voice => (
            tr("音色", "Voice").to_string(),
            if workspace.draft.voice.is_empty() {
                tr("模型默认", "Model default").to_string()
            } else {
                workspace.draft.voice.clone()
            },
        ),
        Field::Language => (
            tr("语言", "Language").to_string(),
            if workspace.draft.language.is_empty() {
                tr("模型默认", "Model default").to_string()
            } else {
                workspace.draft.language.clone()
            },
        ),
        Field::Params => (
            tr("模型参数", "Parameters").to_string(),
            if !params_editable(config, &workspace.draft.engine) {
                tr("此模型无独立参数", "No separate parameters").to_string()
            } else if workspace.draft.params.is_empty() {
                tr("默认", "Default").to_string()
            } else {
                workspace.draft.params.clone()
            },
        ),
        Field::Save => (
            String::new(),
            tr("[ 保存配置 ]", "[ Save configuration ]").to_string(),
        ),
        Field::Follow => (
            String::new(),
            tr(
                "[ 删除单书配置，沿用全局 ]",
                "[ Remove override; follow global ]",
            )
            .to_string(),
        ),
    }
}

fn tr<'a>(zh: &'a str, en: &'a str) -> &'a str {
    match current() {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Apply a confirmed draft. Kept here so the UI's scope semantics have one
/// implementation and can be exercised without synthesis or downloads.
pub fn save(config: &mut Config, request: &Save) -> Result<String, &'static str> {
    let Some(spec) = config.spec(&request.draft.engine) else {
        return Err(tr("模型不存在", "model does not exist"));
    };
    if !install::model_is_ready(&request.draft.engine, spec) {
        return Err(tr(
            "模型尚未下载或校验失败",
            "model is not downloaded or failed validation",
        ));
    }
    validate_params(config, &request.draft.engine, &request.draft.params)?;
    let scope = match request.scope {
        DraftScope::Global => Scope::Default,
        DraftScope::Book => {
            let Some((id, title)) = request.book.as_ref() else {
                return Err(tr("没有可配置的书", "there is no book to configure"));
            };
            Scope::Book { id, title }
        }
    };
    config.set_engine(scope, &request.draft.engine);
    config.set_voice_name(scope, &request.draft.voice);
    config.set_language(scope, &request.draft.language);
    config.set_voice_params(scope, &request.draft.params);
    Ok(match request.scope {
        DraftScope::Global => tr("已保存全局 Voice 配置", "Saved global Voice configuration"),
        DraftScope::Book => tr(
            "已保存单书 Voice 配置",
            "Saved per-book Voice configuration",
        ),
    }
    .to_string())
}

fn params_editable(config: &Config, engine: &str) -> bool {
    engine == "moss"
        || config
            .spec(engine)
            .is_some_and(|spec| spec.serve.trim().is_empty())
}

fn params_hint(engine: &str) -> &'static str {
    match engine {
        "moss" => "temperature=0.8 top_p=0.95 top_k=25 repetition_penalty=1.2 max_tokens=375",
        _ => tr(
            "输入该模型命令支持的附加参数",
            "Enter extra arguments supported by this model command",
        ),
    }
}

fn validate_params(config: &Config, engine: &str, raw: &str) -> Result<(), &'static str> {
    if raw.trim().is_empty() {
        return Ok(());
    }
    if !params_editable(config, engine) {
        return Err(tr(
            "这个常驻模型没有独立的附加参数",
            "this resident model has no separate extra parameters",
        ));
    }
    if engine != "moss" {
        return Ok(());
    }
    let allowed = [
        "temperature",
        "top_p",
        "top_k",
        "repetition_penalty",
        "max_tokens",
    ];
    for token in raw.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            return Err(tr(
                "MOSS 参数必须写成 key=value",
                "MOSS parameters must use key=value",
            ));
        };
        if !allowed.contains(&key) || value.parse::<f32>().is_err() {
            return Err(tr("MOSS 参数无效", "invalid MOSS parameter"));
        }
    }
    Ok(())
}
