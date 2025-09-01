use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{self, Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, Tabs, Wrap},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App) {
    let [top_area, main_panel_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas::<2>(frame.area());

    let [tab_area, _] = Layout::horizontal([Constraint::Length(19), Constraint::Min(0)]).areas::<2>(top_area);

    let tabs: Tabs<'_> = app
        .tabs
        .titles
        .iter()
        .map(|t| text::Line::from(Span::styled(*t, Style::default().fg(Color::Green))))
        .collect::<Tabs>()
        .block(Block::bordered())
        .highlight_style(Style::default().fg(Color::Yellow))
        .select(app.tabs.index);
    frame.render_widget(tabs, tab_area);
    match app.tabs.index {
        0 => draw_first_tab(frame, app, main_panel_area),
        1 => draw_second_tab(frame, app, main_panel_area),
        _ => {}
    }
}

fn draw_first_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    draw_logs(frame, app, area);
}

fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    // let total_lines = app.logs_buffer.len();
    // app.vertical_scroll_state = app.vertical_scroll_state.content_length(total_lines);

    // // Account for the bordered paragraph: inner height excludes the top/bottom borders
    // let inner_height = area.height.saturating_sub(2) as usize;
    // let max_scroll = total_lines.saturating_sub(inner_height);

    // app.vertical_scroll = max_scroll;
    // app.vertical_scroll_state = app.vertical_scroll_state.position(inner_height);

    // #[allow(clippy::cast_possible_truncation)]
    // let paragraph: Paragraph<'_> = Paragraph::new(app.logs_buffer.clone())
    //     // .wrap(Wrap {
    //     // trim: true,
    //     // })
    //     .gray()
    //     .block(Block::bordered())
    //     .scroll((app.vertical_scroll as u16, 0));
    // frame.render_widget(paragraph, area);
    // frame.render_stateful_widget(
    //     Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(Some("↑")).end_symbol(Some("↓")),
    //     area,
    //     &mut app.vertical_scroll_state,
    // );
    // Update scrollbar with full content length
    let total_lines = app.logs_buffer.len();
    app.vertical_scroll_state = app.vertical_scroll_state.content_length(total_lines);

    // Account for the bordered paragraph: inner height excludes the top/bottom borders
    let inner_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(inner_height);

    if app.follow_tail {
        app.vertical_scroll = max_scroll;
        app.vertical_scroll_state = app.vertical_scroll_state.position(app.vertical_scroll);
    } else if app.vertical_scroll > max_scroll {
        // Clamp if content shrank or viewport grew
        app.vertical_scroll = max_scroll;
        app.vertical_scroll_state = app.vertical_scroll_state.position(app.vertical_scroll);
    }

    let start = app.vertical_scroll;
    let end = start.saturating_add(inner_height).min(total_lines);
    let visible: Vec<Line> = app.logs_buffer.iter().skip(start).take(end.saturating_sub(start)).cloned().collect();
    // let visible: Vec<Line> = app
    //     .logs_buffer
    //     .iter()
    //     .skip(start)
    //     .take(end.saturating_sub(start))
    //     // Borrow the stored String; no extra allocation per draw
    //     // .map(|s| Line::from(s.as_str()))
    //     .map(|s| s.clone())
    //     .collect();

    let paragraph = Paragraph::new(visible)
        // .wrap(Wrap {
        //     trim: true,
        // })
        .gray()
        .block(Block::bordered());
    // .scroll((app.vertical_scroll as u16, 0));
    frame.render_widget(paragraph, area);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(Some("↑")).end_symbol(Some("↓")),
        area,
        &mut app.vertical_scroll_state,
    );
}

fn draw_second_tab(frame: &mut Frame, _app: &mut App, area: Rect) {
    let [top, bottom] = Layout::vertical([Constraint::Length(31), Constraint::Min(0)]).areas::<2>(area);

    let block: Block<'_> = Block::bordered();
    frame.render_widget(block, top);
    draw_footer(frame, bottom);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let text = vec![text::Line::from(
        "This is a paragraph with several lines. You can change style your text the way you want",
    )];
    let block = Block::bordered()
        .title(Span::styled("Footer", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap {
        trim: true,
    });
    frame.render_widget(paragraph, area);
}
