mod view;

use std::{
    collections::HashMap,
    io,
    process::ExitCode,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use ratatui::layout::Rect;
use ratcn::{
    InputState, Toast, ToasterState,
    runtime::{Event, EventResult, FocusState, KeyCode, KeyEvent, Ratcn},
    terminal::{Session, SessionEvent, SessionOptions},
};

use crate::{
    lane::{Ctx, Failure, Lane},
    verb::{self, AfterSuccess, Handled, Target, Verb},
};

pub(super) const SPINNER_TICK: Duration = Duration::from_millis(80);

type NewLaneFn = fn(&Ctx, &str, Option<&str>) -> Handled;

enum Mode {
    Browse,
    NewLane {
        verb: &'static Verb,
        create: NewLaneFn,
        name: InputState,
        base: InputState,
    },
    ConfirmForceRm {
        verb: &'static Verb,
        lane: Lane,
        output: String,
    },
    Running {
        verb: &'static Verb,
        lane: Option<Lane>,
        started: Duration,
        done: Receiver<Handled>,
    },
    Failed {
        title: String,
        output: String,
        scroll: u16,
    },
}

#[derive(Clone)]
enum Msg {
    Focus(FocusState),
    Select(String),
    Press(&'static Verb),
    Name(InputState),
    Base(InputState),
    Submit,
    Force,
    Scroll(u16),
    Cancel,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

struct State {
    ctx: Ctx,
    lanes: Vec<Lane>,
    commits: HashMap<String, Vec<String>>,
    selected: Option<String>,
    mode: Mode,
    focus: FocusState,
    toasts: ToasterState<'static>,
    screen: Rect,
}

pub struct App {
    state: State,
    ratcn: Ratcn<State, Msg>,
}

/// Opens the popup. Outside a git repo `lane ls` fails, and that error goes to
/// the shell before the screen takes over.
pub fn run(ctx: Ctx) -> ExitCode {
    let lanes = match ctx.lanes() {
        Ok(lanes) => lanes,
        Err(failure) => {
            eprint!("{}", failure.output());
            return ExitCode::FAILURE;
        }
    };
    match event_loop(App::new(ctx, lanes)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lane-menu: {error}");
            ExitCode::FAILURE
        }
    }
}

fn event_loop(mut app: App) -> io::Result<()> {
    let clock = Instant::now();
    let mut session = Session::open(SessionOptions::new().mouse().adaptive())?;
    loop {
        if app.poll(clock.elapsed()) == Flow::Quit {
            return Ok(());
        }
        let now = clock.elapsed();
        let theme = session.theme();
        session
            .terminal_mut()
            .draw(|frame| app.draw(frame, &theme, now))?;
        if let Some(SessionEvent::Input(input)) = session.next(app.state.timeout(now))?
            && let Ok(event) = Event::try_from(input)
            && app.handle(event, clock.elapsed()) == Flow::Quit
        {
            return Ok(());
        }
    }
}

impl App {
    pub fn new(ctx: Ctx, lanes: Vec<Lane>) -> Self {
        let mut state = State {
            ctx,
            lanes: Vec::new(),
            commits: HashMap::new(),
            selected: None,
            mode: Mode::Browse,
            focus: FocusState::default(),
            toasts: ToasterState::new(),
            screen: Rect::ZERO,
        };
        state.show(lanes);
        Self {
            state,
            ratcn: Ratcn::new().focus(|state: &State| &state.focus, Msg::Focus),
        }
    }

    pub fn handle(&mut self, event: Event, now: Duration) -> Flow {
        if let Event::Key(key) = &event
            && key.modifiers.ctrl
            && key.code == KeyCode::Char('c')
        {
            return Flow::Quit;
        }
        if matches!(self.state.mode, Mode::Running { .. }) {
            return Flow::Continue;
        }
        let msg = match event {
            Event::Key(key) if !key.modifiers.ctrl && !key.modifiers.alt => {
                self.state.shortcut(key)
            }
            _ => None,
        };
        match msg {
            Some(msg) => self.state.update(msg, now),
            None => match self.ratcn.handle_event(event, &self.state) {
                EventResult::Emit(msg) => self.state.update(msg, now),
                EventResult::Consumed | EventResult::Ignored => Flow::Continue,
            },
        }
    }

    /// Applies the running job's result once it has finished.
    pub fn poll(&mut self, now: Duration) -> Flow {
        self.state.poll(now)
    }

    pub fn draw(&mut self, frame: &mut ratatui::Frame, theme: &ratcn::Theme, now: Duration) {
        view::draw(frame, &mut self.state, &mut self.ratcn, theme, now);
    }
}

impl State {
    fn selected_lane(&self) -> Option<&Lane> {
        let name = self.selected.as_ref()?;
        self.lanes.iter().find(|lane| &lane.name == name)
    }

    /// Keys the app owns in each mode, read before any component sees them.
    fn shortcut(&self, key: KeyEvent) -> Option<Msg> {
        match (&self.mode, key.code) {
            (Mode::Browse, KeyCode::Char('q') | KeyCode::Esc) => Some(Msg::Quit),
            (Mode::Browse, KeyCode::Char(key)) => verb::by_key(key).map(Msg::Press),
            (Mode::ConfirmForceRm { .. }, KeyCode::Char('y')) => Some(Msg::Force),
            (Mode::ConfirmForceRm { .. }, KeyCode::Char('n')) => Some(Msg::Cancel),
            (Mode::Failed { .. }, KeyCode::Enter | KeyCode::Esc) => Some(Msg::Cancel),
            (Mode::Failed { output, scroll, .. }, KeyCode::Char('j') | KeyCode::Down) => {
                let max = view::max_scroll(self.screen, output);
                Some(Msg::Scroll(scroll.saturating_add(1).min(max)))
            }
            (Mode::Failed { scroll, .. }, KeyCode::Char('k') | KeyCode::Up) => {
                Some(Msg::Scroll(scroll.saturating_sub(1)))
            }
            _ => None,
        }
    }

    fn update(&mut self, msg: Msg, now: Duration) -> Flow {
        match msg {
            Msg::Focus(focus) => self.focus = focus,
            Msg::Select(name) => self.selected = Some(name),
            Msg::Press(verb) => self.press(verb, now),
            Msg::Name(state) => {
                if let Mode::NewLane { name, .. } = &mut self.mode {
                    *name = state;
                }
            }
            Msg::Base(state) => {
                if let Mode::NewLane { base, .. } = &mut self.mode {
                    *base = state;
                }
            }
            Msg::Submit => {
                if let Mode::NewLane {
                    verb,
                    create,
                    name,
                    base,
                } = &self.mode
                {
                    let (verb, create) = (*verb, *create);
                    let name = name.value().trim().to_owned();
                    let base = Some(base.value().trim().to_owned()).filter(|base| !base.is_empty());
                    if !name.is_empty() {
                        self.start(verb, None, now, move |ctx| {
                            create(ctx, &name, base.as_deref())
                        });
                    }
                }
            }
            Msg::Force => {
                if let Mode::ConfirmForceRm { verb, lane, .. } = &self.mode {
                    let (verb, lane) = (*verb, lane.clone());
                    let target = lane.clone();
                    self.start(verb, Some(lane), now, move |ctx| {
                        verb::rm_force(ctx, &target)
                    });
                }
            }
            Msg::Scroll(offset) => {
                if let Mode::Failed { scroll, .. } = &mut self.mode {
                    *scroll = offset;
                }
            }
            Msg::Cancel => self.enter(Mode::Browse),
            Msg::Quit => return Flow::Quit,
        }
        Flow::Continue
    }

    fn press(&mut self, verb: &'static Verb, now: Duration) {
        let selected = self.selected_lane().cloned();
        if !verb.target.accepts(selected.as_ref()) {
            return;
        }
        match verb.target {
            Target::None(handler) => self.start(verb, None, now, handler),
            Target::NewLane(create) => self.enter(Mode::NewLane {
                verb,
                create,
                name: InputState::default(),
                base: InputState::default(),
            }),
            Target::AnyLane(handler) | Target::ClosedLane(handler) => {
                if let Some(lane) = selected {
                    let target = lane.clone();
                    self.start(verb, Some(lane), now, move |ctx| handler(ctx, &target));
                }
            }
        }
    }

    /// Runs `job` on a worker thread so the spinner keeps turning.
    fn start(
        &mut self,
        verb: &'static Verb,
        lane: Option<Lane>,
        now: Duration,
        job: impl FnOnce(&Ctx) -> Handled + Send + 'static,
    ) {
        let (send, done) = mpsc::channel();
        let ctx = self.ctx.clone();
        thread::spawn(move || send.send(job(&ctx)));
        self.enter(Mode::Running {
            verb,
            lane,
            started: now,
            done,
        });
    }

    /// Every mode change hands focus back to the runtime's default, which is
    /// the first control of whatever is on top: a dialog, or the lane list.
    fn enter(&mut self, mode: Mode) {
        self.mode = mode;
        self.focus = FocusState::default();
    }

    fn poll(&mut self, now: Duration) -> Flow {
        let Mode::Running {
            verb, lane, done, ..
        } = &mut self.mode
        else {
            return Flow::Continue;
        };
        let Ok(result) = done.try_recv() else {
            return Flow::Continue;
        };
        let (verb, lane) = (*verb, lane.take());
        let name = lane
            .as_ref()
            .map_or_else(String::new, |lane| format!(" {}", lane.name));
        match (result, verb.after, lane) {
            (Ok(_), AfterSuccess::ExitPopup, _) => return Flow::Quit,
            (Ok(outcome), AfterSuccess::Stay { done }, _) => {
                let mut toast = Toast::success(format!("{done}{name}"));
                if let Some(line) = outcome
                    .output
                    .lines()
                    .map(str::trim)
                    .rfind(|line| !line.is_empty())
                {
                    toast = toast.with_description(line.to_owned());
                }
                self.toasts.push(toast, now);
                self.enter(Mode::Browse);
            }
            (Err(Failure::Refused(output)), _, Some(lane)) => {
                self.enter(Mode::ConfirmForceRm { verb, lane, output });
            }
            (Err(failure), _, _) => self.enter(Mode::Failed {
                title: format!("{}{name} failed", verb.name),
                output: failure.output().to_owned(),
                scroll: 0,
            }),
        }
        self.refresh();
        Flow::Continue
    }

    /// How long the loop may sleep before something on screen changes by itself.
    fn timeout(&self, now: Duration) -> Option<Duration> {
        match self.mode {
            Mode::Running { .. } => Some(SPINNER_TICK),
            _ => self.toasts.time_until_next_expiry(now),
        }
    }

    fn refresh(&mut self) {
        match self.ctx.lanes() {
            Ok(lanes) => self.show(lanes),
            Err(failure) => self.enter(Mode::Failed {
                title: "could not list lanes".to_owned(),
                output: failure.output().to_owned(),
                scroll: 0,
            }),
        }
    }

    /// Replaces the lane list, keeping the selection on the same lane while it exists.
    fn show(&mut self, lanes: Vec<Lane>) {
        self.commits = lanes
            .iter()
            .map(|lane| (lane.name.clone(), self.ctx.recent_commits(lane)))
            .collect();
        self.lanes = lanes;
        if self.selected_lane().is_none() {
            self.selected = self
                .lanes
                .iter()
                .find(|lane| self.ctx.is_current(lane))
                .or(self.lanes.first())
                .map(|lane| lane.name.clone());
        }
    }
}
