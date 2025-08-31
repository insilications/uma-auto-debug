use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::scrollbar,
    text::{self, Line, Masked, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs, Wrap},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(frame.area());

    let chunks_tab = Layout::horizontal([Constraint::Length(19), Constraint::Min(0)]).split(chunks[0]);

    let tabs = app
        .tabs
        .titles
        .iter()
        .map(|t| text::Line::from(Span::styled(*t, Style::default().fg(Color::Green))))
        .collect::<Tabs>()
        // .block(Block::new().borders(Borders::TOP).title(app.title))
        .block(Block::bordered())
        .highlight_style(Style::default().fg(Color::Yellow))
        .select(app.tabs.index);
    frame.render_widget(tabs, chunks_tab[0]);
    match app.tabs.index {
        0 => draw_first_tab(frame, app, chunks[1]),
        1 => draw_second_tab(frame, app, chunks[1]),
        _ => {}
    }
}

fn draw_first_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Min(0)]).split(area);

    draw_logs(frame, app, chunks[0]);
}

fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    // let block = Block::bordered();

    let s = "Veeeeeeeeeeeeeeeery    loooooooooooooooooong   striiiiiiiiiiiiiiiiiiiiiiiiiing.   ";
    let mut long_line = s.repeat(usize::from(area.width) / s.len() + 4);
    long_line.push('\n');

    let text = vec![
        Line::from("This is a line "),
        Line::from("This is a line   ".red()),
        Line::from("This is a line".on_dark_gray()),
        Line::from("This is a longer line".crossed_out()),
        Line::from(long_line.clone()),
        Line::from("This is a line".reset()),
        Line::from(vec![
            Span::raw("Masked text: "),
            Span::styled(Masked::new("password", '*'), Style::new().fg(Color::Red)),
        ]),
        Line::from("This is a line "),
        Line::from("This is a line   ".red()),
        Line::from("This is a line".on_dark_gray()),
        Line::from("This is a longer line".crossed_out()),
        Line::from(long_line.clone()),
        Line::from("This is a line".reset()),
        Line::from(vec![
            Span::raw("Masked text: "),
            Span::styled(Masked::new("password", '*'), Style::new().fg(Color::Red)),
        ]),
    ];
    app.vertical_scroll_state = app.vertical_scroll_state.content_length(text.len());
    app.horizontal_scroll_state = app.horizontal_scroll_state.content_length(long_line.len());

    let create_block = |title: &'static str| Block::bordered().gray().title(title.bold());

    // let paragraph = Paragraph::new(text.clone())
    //     .wrap(Wrap {
    //         trim: true,
    //     })
    //     .gray()
    //     // .block(create_block("Vertical scrollbar without arrows, without track symbol and mirrored"))
    //     .block(Block::bordered())
    //     .scroll((app.vertical_scroll as u16, 0));
    // frame.render_widget(paragraph, area);
    // frame.render_stateful_widget(
    //     Scrollbar::new(ScrollbarOrientation::VerticalLeft)
    //         .symbols(scrollbar::VERTICAL)
    //         .begin_symbol(None)
    //         .track_symbol(None)
    //         .end_symbol(None),
    //     area.inner(Margin {
    //         vertical: 1,
    //         horizontal: 0,
    //     }),
    //     &mut app.vertical_scroll_state,
    // );

    let paragraph =
        Paragraph::new(text.clone()).gray().block(Block::bordered()).scroll((app.vertical_scroll as u16, 0));
    frame.render_widget(paragraph, area);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(Some("↑")).end_symbol(Some("↓")),
        area,
        &mut app.vertical_scroll_state,
    );

    // frame.render_widget(block, area);
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

fn draw_second_tab(frame: &mut Frame, _app: &mut App, area: Rect) {
    let chunks = Layout::vertical([Constraint::Length(31), Constraint::Min(0)]).split(area);

    let block = Block::bordered();
    frame.render_widget(block, chunks[0]);
    draw_footer(frame, chunks[1]);
}
