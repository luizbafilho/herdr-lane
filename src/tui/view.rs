use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Padding, Paragraph},
};
use ratcn::{
    Button, Dialog, Input, InputState, List, ListItem, ScrollArea, Theme, ToasterWidget,
    runtime::{DeclareCtx, Ratcn},
    text_width::{display_width, wrap_to_width},
};

use super::{Mode, Msg, SPINNER_TICK, State};
use crate::{lane::Lane, verb::VERBS};

/// Below this width the detail panel gives its room to the lane list.
const DETAIL_MIN_WIDTH: u16 = 100;
const DETAIL_WIDTH: u16 = 44;
/// Rows a dialog spends outside its content: border and padding on both
/// sides, the gap above the action row, and the one-row action row.
const DIALOG_CHROME: u16 = 6;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

type Ctx<'a> = DeclareCtx<'a, State, Msg>;

pub(super) fn draw(
    frame: &mut Frame,
    state: &mut State,
    ratcn: &mut Ratcn<State, Msg>,
    theme: &Theme,
    now: Duration,
) {
    let area = frame.area();
    state.screen = area;
    state.toasts.prune_expired(now);
    let [title, body, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let theme = *theme;
    ratcn.render(frame, area, state, &theme, |ctx| {
        ctx.paint_widget(
            Block::new().style(Style::new().bg(theme.background).fg(theme.foreground)),
            area,
        );
        title_bar(ctx, &theme, title);
        lanes_panel(ctx, &theme, body);
        ctx.paint_widget(key_bar(ctx.state(), &theme), keys);
        modal(ctx, &theme, area, now);
    });
    frame.render_widget(ToasterWidget::new(&state.toasts, now).themed(&theme), body);
}

fn title_bar(ctx: &mut Ctx<'_>, theme: &Theme, area: Rect) {
    let state = ctx.state();
    let count = match state.lanes.len() {
        1 => "1 lane ".to_owned(),
        n => format!("{n} lanes "),
    };
    let repo = Line::from(vec![
        Span::styled(
            " lane ",
            Style::new()
                .fg(theme.primary_foreground)
                .bg(theme.primary)
                .bold(),
        ),
        Span::styled(format!(" {}", state.ctx.repo_name()), Style::new().bold()),
    ]);
    let [left, right] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(u16::try_from(count.len()).unwrap_or(u16::MAX)),
    ])
    .areas(area);
    ctx.paint_widget(Paragraph::new(repo), left);
    ctx.paint_widget(Paragraph::new(count).fg(theme.muted_foreground), right);
}

fn key_bar(state: &State, theme: &Theme) -> Paragraph<'static> {
    let selected = state.selected_lane();
    let mut spans = vec![Span::raw(" ")];
    for verb in &VERBS {
        let (key, label) = if verb.target.accepts(selected) {
            (
                Style::new().fg(theme.accent).bold(),
                Style::new().fg(theme.foreground),
            )
        } else {
            let off = Style::new()
                .fg(theme.muted_foreground)
                .add_modifier(Modifier::DIM);
            (off, off)
        };
        spans.extend([
            Span::styled(verb.key.to_string(), key),
            Span::styled(format!(" {}   ", verb.label), label),
        ]);
    }
    spans.extend([
        Span::styled("q", Style::new().fg(theme.accent).bold()),
        Span::styled(" quit", Style::new().fg(theme.foreground)),
    ]);
    Paragraph::new(Line::from(spans))
}

fn lanes_panel(ctx: &mut Ctx<'_>, theme: &Theme, area: Rect) {
    let state = ctx.state();
    if state.lanes.is_empty() {
        ctx.paint_widget(panel(theme, " lanes "), area);
        ctx.paint_widget(empty_state(theme), panel(theme, " lanes ").inner(area));
        return;
    }
    let (list_area, detail_area) = if area.width >= DETAIL_MIN_WIDTH {
        let [list, detail] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(DETAIL_WIDTH)]).areas(area);
        (list, Some(detail))
    } else {
        (area, None)
    };

    let block = panel(theme, " lanes ").title_bottom(
        Line::from(vec![
            Span::styled(" > ", Style::new().fg(theme.accent)),
            Span::raw("current  "),
            Span::styled("* ", Style::new().fg(theme.primary)),
            Span::raw("open in herdr "),
        ])
        .fg(theme.muted_foreground)
        .right_aligned(),
    );
    let inner = block.inner(list_area);
    ctx.paint_widget(block, list_area);
    let [header, rows] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
    let columns = Columns::fit(&state.lanes, inner.width);
    ctx.paint_widget(Paragraph::new(columns.header(theme)), header);

    let items = state
        .lanes
        .iter()
        .map(|lane| ListItem::new(lane.name.clone(), lane.name.clone()));
    let theme = *theme;
    ctx.component(
        "lanes",
        List::new(items)
            .item_focus(
                |state: &State| state.selected.clone(),
                |name, _| Msg::Select(name),
            )
            .paint_item(move |state: &State, row| {
                let lane = &state.lanes[row.index];
                columns.row(lane, state.ctx.is_current(lane), &theme)
            }),
        rows,
    );

    if let (Some(detail), Some(lane)) = (detail_area, state.selected_lane()) {
        let commits = state.commits.get(&lane.name).map_or(&[][..], Vec::as_slice);
        ctx.paint_widget(detail_panel(state, lane, commits, &theme), detail);
    }
}

fn panel(theme: &Theme, title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.border))
        .title(Line::from(title.to_owned()).fg(theme.muted_foreground))
        .padding(Padding::horizontal(1))
}

fn empty_state(theme: &Theme) -> Paragraph<'static> {
    Paragraph::new(Text::from(vec![
        Line::raw(""),
        Line::from("No lanes yet").bold(),
        Line::from(vec![
            Span::raw("Press "),
            Span::styled("n", Style::new().fg(theme.accent).bold()),
            Span::raw(" to start one."),
        ])
        .fg(theme.muted_foreground),
    ]))
    .centered()
}

/// Column widths for the lane table, fitted to the lanes on screen.
#[derive(Debug, Clone, Copy)]
struct Columns {
    name: usize,
    /// Zero when the panel is too narrow to show branches.
    branch: usize,
    state: usize,
}

impl Columns {
    const MAX: usize = 28;

    fn fit(lanes: &[Lane], width: u16) -> Self {
        let widest = |header: &str, cell: fn(&Lane) -> &str| {
            lanes
                .iter()
                .map(|lane| display_width(cell(lane)))
                .fold(header.len(), usize::max)
                .min(Self::MAX)
        };
        let name = widest("NAME", |lane| &lane.name);
        let branch = widest("BRANCH", |lane| &lane.branch);
        let state = widest("STATE", |lane| &lane.state);
        // markers, name, branch, state, dirty badge, notes, with two-cell gaps
        let full = 4 + name + 2 + branch + 2 + state + 2 + 5 + 2 + 5;
        Self {
            name,
            branch: if usize::from(width) >= full {
                branch
            } else {
                0
            },
            state,
        }
    }

    fn cells(
        self,
        name: Span<'static>,
        branch: Span<'static>,
        rest: [Span<'static>; 3],
    ) -> Vec<Span<'static>> {
        let mut spans = vec![pad(name, self.name)];
        if self.branch > 0 {
            spans.push(pad(branch, self.branch));
        }
        let [state, dirty, notes] = rest;
        spans.extend([pad(state, self.state), pad(dirty, 5), notes]);
        spans
    }

    fn header(self, theme: &Theme) -> Line<'static> {
        let label = |text: &'static str| Span::raw(text);
        let mut spans = vec![Span::raw("    ")];
        spans.extend(self.cells(
            label("NAME"),
            label("BRANCH"),
            [label("STATE"), label("DIRTY"), label("NOTES")],
        ));
        Line::from(spans).fg(theme.muted_foreground).bold()
    }

    fn row(self, lane: &Lane, current: bool, theme: &Theme) -> Line<'static> {
        let marker = |on: bool, glyph: &'static str, color: Color| {
            if on {
                Span::styled(glyph, Style::new().fg(color).bold())
            } else {
                Span::raw(" ")
            }
        };
        let state_color = if lane.state == "open" {
            theme.foreground
        } else {
            theme.accent
        };
        let mut spans = vec![
            marker(current, ">", theme.accent),
            Span::raw(" "),
            marker(lane.workspace.is_some(), "*", theme.primary),
            Span::raw(" "),
        ];
        spans.extend(self.cells(
            Span::styled(lane.name.clone(), Style::new().bold()),
            Span::styled(lane.branch.clone(), Style::new().fg(theme.muted_foreground)),
            [
                Span::styled(lane.state.clone(), Style::new().fg(state_color)),
                if lane.dirty {
                    Span::styled("dirty", Style::new().fg(theme.warning))
                } else {
                    Span::raw("")
                },
                match lane.pending_notes {
                    0 => Span::styled("0", Style::new().fg(theme.muted_foreground)),
                    n => Span::styled(n.to_string(), Style::new().fg(theme.warning).bold()),
                },
            ],
        ));
        Line::from(spans)
    }
}

/// `span` cut or padded to exactly `width` cells, then a two-cell gap.
fn pad(span: Span<'static>, width: usize) -> Span<'static> {
    let text = ratcn::text_width::truncate_to_width(&span.content, width);
    let padding = width - display_width(text) + 2;
    Span::styled(format!("{text}{}", " ".repeat(padding)), span.style)
}

fn detail_panel(
    state: &State,
    lane: &Lane,
    commits: &[String],
    theme: &Theme,
) -> Paragraph<'static> {
    let field = |label: &'static str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(
                format!("{label:<9}"),
                Style::new().fg(theme.muted_foreground),
            ),
            value,
        ])
    };
    let path = lane
        .path
        .strip_prefix(&state.ctx.repo)
        .unwrap_or(&lane.path);
    let mut lines = vec![
        field("path", Span::raw(path.display().to_string())),
        field("branch", Span::raw(lane.branch.clone())),
        field("state", Span::raw(lane.state.clone())),
        field(
            "changes",
            if lane.dirty {
                Span::styled("uncommitted", Style::new().fg(theme.warning))
            } else {
                Span::raw("clean")
            },
        ),
        field(
            "notes",
            match lane.pending_notes {
                0 => Span::raw("none pending"),
                n => Span::styled(format!("{n} pending"), Style::new().fg(theme.warning)),
            },
        ),
        field(
            "herdr",
            match &lane.workspace {
                Some(id) => Span::styled(format!("open in {id}"), Style::new().fg(theme.primary)),
                None => Span::styled("not open", Style::new().fg(theme.muted_foreground)),
            },
        ),
        Line::raw(""),
        Line::from("recent commits")
            .fg(theme.muted_foreground)
            .bold(),
    ];
    lines.extend(commits.iter().map(|commit| match commit.split_once(' ') {
        Some((hash, subject)) => Line::from(vec![
            Span::styled(hash.to_owned(), Style::new().fg(theme.accent)),
            Span::raw(format!(" {subject}")),
        ]),
        None => Line::raw(commit.clone()),
    }));
    Paragraph::new(lines).block(panel(theme, &format!(" {} ", lane.name)))
}

fn modal(ctx: &mut Ctx<'_>, theme: &Theme, area: Rect, now: Duration) {
    let state = ctx.state();
    match &state.mode {
        Mode::Browse => {}
        Mode::NewLane { name, .. } => {
            let empty = name.value().trim().is_empty();
            let dialog = Dialog::new()
                .title("New lane")
                .on_dismiss(|| Msg::Cancel)
                .content(6, |ctx| {
                    let [name, base] =
                        Layout::vertical([Constraint::Length(3), Constraint::Length(3)])
                            .areas(ctx.area());
                    ctx.component(
                        "name",
                        Input::new()
                            .value(|state: &State| form(state).0, Msg::Name)
                            .title("Name")
                            .placeholder("fix-login")
                            .on_submit(|| Msg::Submit),
                        name,
                    );
                    ctx.component(
                        "base",
                        Input::new()
                            .value(|state: &State| form(state).1, Msg::Base)
                            .title("Base ref")
                            .placeholder("the repo's default base")
                            .on_submit(|| Msg::Submit),
                        base,
                    );
                })
                .action(
                    "cancel",
                    Button::new("Cancel").secondary().on_press(|| Msg::Cancel),
                )
                .action(
                    "create",
                    Button::new("Create")
                        .disabled(empty)
                        .on_press(|| Msg::Submit),
                );
            ctx.modal("new-lane", dialog, area);
        }
        Mode::ConfirmForceRm { lane, output, .. } => {
            let dialog = Dialog::new()
                .title(format!("Discard {}?", lane.name))
                .description(format!(
                    "{}\n\nlane rm kept this lane. Discarding it deletes its uncommitted work and pending notes.",
                    output.trim_end()
                ))
                .on_dismiss(|| Msg::Cancel)
                .action("keep", Button::new("Keep  n").secondary().on_press(|| Msg::Cancel))
                .action(
                    "discard",
                    Button::new("Discard  y").destructive().on_press(|| Msg::Force),
                );
            ctx.modal("confirm-force-rm", dialog, area);
        }
        Mode::Running {
            verb,
            lane,
            started,
            ..
        } => {
            let elapsed = now.saturating_sub(*started);
            let tick = (elapsed.as_millis() / SPINNER_TICK.as_millis()) as usize;
            let frame = SPINNER[tick % SPINNER.len()];
            let command = match lane {
                Some(lane) => format!("lane {} {}", verb.name, lane.name),
                None => format!("lane {}", verb.name),
            };
            let line = Line::from(vec![
                Span::styled(format!("{frame} "), Style::new().fg(theme.accent)),
                Span::raw(command),
                Span::styled(
                    format!("  {}s", elapsed.as_secs()),
                    Style::new().fg(theme.muted_foreground),
                ),
            ]);
            let dialog = Dialog::new().title("Working").content(1, move |ctx| {
                ctx.paint_widget(Paragraph::new(line), ctx.area())
            });
            ctx.modal("running", dialog, area);
        }
        Mode::Failed { title, output, .. } => {
            let failed = Failed::layout(area, output);
            let text: Vec<Line<'static>> = failed.lines.into_iter().map(Line::from).collect();
            let rows = u16::try_from(text.len()).unwrap_or(u16::MAX);
            let dialog = Dialog::new()
                .title(title.clone())
                .outer_width(failed.width)
                .on_dismiss(|| Msg::Cancel)
                .content(failed.visible, move |ctx| {
                    ctx.component(
                        "output",
                        ScrollArea::new(rows)
                            .scroll(
                                |state: &State| match state.mode {
                                    Mode::Failed { scroll, .. } => scroll,
                                    _ => 0,
                                },
                                Msg::Scroll,
                            )
                            .content(move |ctx| {
                                ctx.paint_widget(Paragraph::new(text), ctx.area());
                            }),
                        ctx.area(),
                    );
                })
                .action("close", Button::new("Close").on_press(|| Msg::Cancel));
            ctx.modal("failed", dialog, area);
        }
    }
}

/// The new-lane inputs. Only read while that dialog is declared.
fn form(state: &State) -> (&InputState, &InputState) {
    match &state.mode {
        Mode::NewLane { name, base, .. } => (name, base),
        _ => unreachable!("the new-lane inputs are declared only in Mode::NewLane"),
    }
}

/// The failure dialog's geometry, shared by painting and by `j`/`k` scrolling.
struct Failed {
    width: u16,
    lines: Vec<String>,
    visible: u16,
}

impl Failed {
    fn layout(screen: Rect, output: &str) -> Self {
        let width = screen.width.saturating_sub(4).min(100);
        // Inside the border and padding, less the scrollbar column.
        let text_width = usize::from(width.saturating_sub(5)).max(1);
        let lines: Vec<String> = output
            .trim_end()
            .lines()
            .flat_map(|line| {
                let body = line.trim_start();
                let indent = &line[..line.len() - body.len()];
                wrap_to_width(body, text_width.saturating_sub(indent.len()).max(1))
                    .into_iter()
                    .map(move |row| format!("{indent}{row}"))
            })
            .collect();
        let count = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        Self {
            width,
            visible: count.clamp(1, screen.height.saturating_sub(DIALOG_CHROME).max(1)),
            lines,
        }
    }
}

pub(super) fn max_scroll(screen: Rect, output: &str) -> u16 {
    let failed = Failed::layout(screen, output);
    u16::try_from(failed.lines.len())
        .unwrap_or(u16::MAX)
        .saturating_sub(failed.visible)
}
