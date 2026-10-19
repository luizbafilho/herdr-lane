#[allow(dead_code, reason = "each test crate uses part of the shared fixture")]
mod support;

use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use lane_menu::{
    lane::{Ctx, Lane},
    tui::{App, Flow},
};
use ratatui::{Terminal, backend::TestBackend, style::Modifier};
use ratcn::{
    Theme,
    runtime::{Event, KeyCode, KeyEvent, Modifiers},
};
use support::Repo;

struct Screen {
    terminal: Terminal<TestBackend>,
    app: App,
    clock: Instant,
}

impl Screen {
    fn new(app: App, width: u16, height: u16) -> Self {
        let mut screen = Self {
            terminal: Terminal::new(TestBackend::new(width, height)).unwrap(),
            app,
            clock: Instant::now(),
        };
        screen.draw();
        screen
    }

    fn draw(&mut self) {
        let now = self.clock.elapsed();
        let app = &mut self.app;
        self.terminal
            .draw(|frame| app.draw(frame, &Theme::default_dark(), now))
            .unwrap();
    }

    fn text(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(&mut self, code: KeyCode) -> Flow {
        self.send(KeyEvent::new(code))
    }

    fn send(&mut self, key: KeyEvent) -> Flow {
        let flow = self.app.handle(Event::Key(key), self.clock.elapsed());
        self.draw();
        flow
    }

    fn typed(&mut self, text: &str) {
        for c in text.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Waits for the running command, then redraws.
    fn settle(&mut self) -> Flow {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.text().contains("Working") {
            assert!(Instant::now() < deadline, "the command never finished");
            thread::sleep(Duration::from_millis(10));
            if self.app.poll(self.clock.elapsed()) == Flow::Quit {
                return Flow::Quit;
            }
            self.draw();
        }
        Flow::Continue
    }

    /// Whether the key bar shows `label` dimmed, which is how a verb reads as disabled.
    fn key_disabled(&self, label: &str) -> bool {
        let buffer = self.terminal.backend().buffer();
        let row = buffer.area.height - 1;
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, row)].symbol())
            .collect();
        let column = line
            .find(label)
            .unwrap_or_else(|| panic!("{label:?} not in {line:?}"));
        buffer[(u16::try_from(column).unwrap(), row)]
            .modifier
            .contains(Modifier::DIM)
    }
}

fn lane(name: &str, workspace: Option<&str>) -> Lane {
    Lane {
        name: name.to_owned(),
        path: PathBuf::from("/work/demo/.lane/trees").join(name),
        branch: name.to_owned(),
        state: "open".to_owned(),
        dirty: false,
        pending_notes: 0,
        workspace: workspace.map(str::to_owned),
    }
}

fn demo(lanes: Vec<Lane>) -> App {
    let ctx = Ctx {
        repo: PathBuf::from("/work/demo"),
        current: Some(PathBuf::from("/work/demo/.lane/trees/feat-b")),
        herdr: PathBuf::from("herdr"),
    };
    App::new(ctx, lanes)
}

fn two_lanes() -> Vec<Lane> {
    let mut a = lane("feat-a", None);
    a.dirty = true;
    a.pending_notes = 3;
    vec![a, lane("feat-b", Some("w7"))]
}

#[test]
fn browse_lists_lanes_with_markers_and_details_the_current_one() {
    let screen = Screen::new(demo(two_lanes()), 120, 30);
    let text = screen.text();

    let title = text.lines().next().unwrap();
    assert!(
        title.contains(" lane  demo") && title.contains("2 lanes"),
        "{title}"
    );
    assert!(text.contains("NAME"), "{text}");
    assert!(
        text.contains("> * feat-b"),
        "current and open markers:\n{text}"
    );
    let row_a = text.lines().find(|line| line.contains("feat-a  ")).unwrap();
    assert!(row_a.contains("dirty") && row_a.contains('3'), "{row_a}");
    assert!(
        text.contains("open in w7"),
        "detail follows the current lane:\n{text}"
    );
    assert!(text.contains(".lane/trees/feat-b"), "{text}");
}

#[test]
fn moving_the_selection_retargets_the_verbs() {
    let mut screen = Screen::new(demo(two_lanes()), 120, 30);
    assert!(
        screen.key_disabled("o open"),
        "feat-b already has a workspace"
    );
    assert!(!screen.key_disabled("m merge"));

    screen.key(KeyCode::Char('k'));

    assert!(!screen.key_disabled("o open"), "feat-a has none");
    assert!(screen.text().contains("uncommitted"), "detail shows feat-a");
}

#[test]
fn a_disabled_verb_does_nothing() {
    let mut screen = Screen::new(demo(two_lanes()), 120, 30);

    assert_eq!(screen.key(KeyCode::Char('o')), Flow::Continue);

    assert!(!screen.text().contains("Working"));
}

#[test]
fn a_narrow_popup_drops_the_detail_panel() {
    let screen = Screen::new(demo(two_lanes()), 70, 20);
    let text = screen.text();

    assert!(text.contains("feat-a") && text.contains("feat-b"), "{text}");
    assert!(!text.contains("recent commits"), "{text}");
}

#[test]
fn an_empty_repo_points_at_n() {
    let screen = Screen::new(demo(Vec::new()), 100, 24);
    let text = screen.text();

    assert!(text.contains("No lanes yet"), "{text}");
    assert!(text.contains("Press n to start one."), "{text}");
    assert!(text.contains("0 lanes"), "{text}");
    assert!(screen.key_disabled("m merge"));
    assert!(!screen.key_disabled("n new"));
    assert!(!screen.key_disabled("x prune"));
}

#[test]
fn the_new_lane_dialog_takes_a_name_and_esc_cancels_it() {
    let mut screen = Screen::new(demo(two_lanes()), 120, 30);

    screen.key(KeyCode::Char('n'));
    screen.typed("spike");
    let text = screen.text();

    assert!(text.contains("New lane"), "{text}");
    assert!(text.contains("Base ref"), "{text}");
    assert!(text.contains("spike"), "{text}");

    assert_eq!(screen.key(KeyCode::Esc), Flow::Continue);
    assert!(!screen.text().contains("New lane"));
    assert_eq!(screen.key(KeyCode::Char('q')), Flow::Quit);
}

#[test]
fn ctrl_c_quits_from_a_dialog() {
    let mut screen = Screen::new(demo(two_lanes()), 120, 30);
    screen.key(KeyCode::Char('n'));

    let ctrl_c = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    };

    assert_eq!(screen.send(ctrl_c), Flow::Quit);
}

#[test]
fn rm_asks_before_forcing_and_then_confirms_with_a_toast() {
    let repo = Repo::new();
    repo.new_lane("wip");
    std::fs::write(repo.lane_path("wip").join("README"), "edited\n").unwrap();
    let mut screen = Screen::new(App::new(repo.ctx.clone(), repo.lanes()), 120, 30);

    screen.key(KeyCode::Char('r'));
    screen.settle();
    let text = screen.text();
    assert!(text.contains("Discard wip?"), "{text}");
    assert!(
        text.contains("uncommitted"),
        "lane's refusal is shown:\n{text}"
    );
    assert_eq!(
        repo.lanes().len(),
        1,
        "nothing is discarded before the answer"
    );

    screen.key(KeyCode::Char('y'));
    screen.settle();
    let text = screen.text();

    assert!(repo.lanes().is_empty());
    assert!(text.contains("removed wip"), "{text}");
    assert!(text.contains("No lanes yet"), "the list refreshed:\n{text}");
}

#[test]
fn a_failed_command_shows_its_output_until_dismissed() {
    let repo = Repo::new();
    repo.new_lane("lonely");
    repo.commit_in("lonely", "change.txt");
    let mut screen = Screen::new(App::new(repo.ctx.clone(), repo.lanes()), 120, 30);

    screen.key(KeyCode::Char('p'));
    screen.settle();
    let text = screen.text();

    assert!(text.contains("push lonely failed"), "{text}");
    assert!(text.contains("no remote configured"), "{text}");
    assert!(text.contains("Close"), "{text}");

    screen.key(KeyCode::Enter);
    assert!(!screen.text().contains("failed"));
}

#[test]
fn long_failure_output_scrolls_with_j_and_k() {
    let repo = Repo::new();
    repo.new_lane("lonely");
    repo.commit_in("lonely", "change.txt");
    let mut screen = Screen::new(App::new(repo.ctx.clone(), repo.lanes()), 40, 12);
    screen.key(KeyCode::Char('p'));
    screen.settle();
    assert!(
        screen.text().contains("rebased onto main"),
        "{}",
        screen.text()
    );

    screen.key(KeyCode::Char('j'));
    let scrolled = screen.text();
    assert!(!scrolled.contains("rebased onto main"), "{scrolled}");
    assert!(
        scrolled.contains("push lonely failed"),
        "j does not dismiss"
    );

    screen.key(KeyCode::Char('k'));
    assert!(screen.text().contains("rebased onto main"));
}
