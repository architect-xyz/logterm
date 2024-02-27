use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, FixedOffset, Utc};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    character::complete::multispace1,
    combinator::{consumed, map},
    sequence::tuple,
};
use regex::Regex;
use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DisplayLine {
    pub lln: usize,      // logical line number
    pub ll: Option<i32>, // log level
    pub ts: Option<DateTime<Utc>>,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub label: SpanLabel,
}

impl Span {
    pub fn noise(text: String) -> Self {
        Span { text, label: SpanLabel::Noise }
    }

    pub fn timestamp(text: String) -> Self {
        Span { text, label: SpanLabel::Timestamp }
    }

    pub fn level(text: String) -> Self {
        Span { text, label: SpanLabel::Level }
    }

    pub fn target(text: String) -> Self {
        Span { text, label: SpanLabel::Target }
    }

    pub fn text(text: String) -> Self {
        Span { text, label: SpanLabel::Text }
    }

    pub fn text_match(text: String) -> Self {
        Span { text, label: SpanLabel::TextMatch }
    }

    pub fn split_at(&self, width: usize) -> Result<(Span, Span)> {
        let glyphs = self.text.graphemes(true).collect::<Vec<&str>>();
        if width > glyphs.len() {
            return Ok((
                Span { text: "".to_string(), label: self.label },
                Span { text: self.text.clone(), label: self.label },
            ));
        }
        let (l, r) = glyphs.split_at(width);
        let l = l.concat();
        let r = r.concat();
        if UnicodeWidthStr::width(l.as_str()) > width {
            bail!("impossible to break span to width {width}");
        }
        Ok((Span { text: l, label: self.label }, Span { text: r, label: self.label }))
    }

    pub fn split_soft_once(&self) -> Option<(Span, Span, Span)> {
        let (l, r) = self.text.split_once(" ")?;
        Some((
            Span { text: l.to_string(), label: self.label },
            Span { text: " ".to_string(), label: self.label },
            Span { text: r.to_string(), label: self.label },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanLabel {
    Noise,
    Timestamp,
    Level,
    Target,
    Text,
    TextMatch,
}

pub struct DisplayLinesBuilder {
    lln: usize,
    ll: Option<i32>,
    ts: Option<DateTime<Utc>>,
    spans: Vec<Span>,
    cum_width: usize,
    lines: Vec<DisplayLine>,
    cols: usize,
}

impl DisplayLinesBuilder {
    pub fn new(lln: usize, cols: usize) -> Self {
        DisplayLinesBuilder {
            lln,
            ll: None,
            ts: None,
            spans: vec![],
            cum_width: 0,
            lines: vec![],
            cols,
        }
    }

    pub fn build(mut self) -> Vec<DisplayLine> {
        if !self.spans.is_empty() {
            self.push_line();
        }
        self.lines
    }

    fn push_line(&mut self) {
        let spans = std::mem::replace(&mut self.spans, vec![]);
        self.cum_width = 0;
        self.lines.push(DisplayLine { lln: self.lln, ll: self.ll, ts: self.ts, spans });
    }

    pub fn push_span(&mut self, span: Span) -> Result<()> {
        if span.text.is_empty() {
            return Ok(());
        }
        let span_width = span.text.width();
        if self.cum_width + span_width > self.cols {
            // the span too wide, try a soft break
            match span.split_soft_once() {
                // CR alee: this will soft break way more often than needed
                Some((l, w, r)) => {
                    self.push_span(l)?;
                    self.push_span(w)?;
                    self.push_span(r)?;
                }
                None => {
                    // no soft break available
                    if span_width > self.cols {
                        // absolutely too wide, hardbreak
                        let (l, r) = span.split_at(self.cols - self.cum_width)?;
                        if !l.text.is_empty() {
                            self.spans.push(l);
                        }
                        self.push_line();
                        self.push_span(r)?;
                    } else {
                        self.push_line();
                        self.push_span(span)?;
                    }
                }
            }
        } else {
            self.cum_width += span_width;
            self.spans.push(span);
            if self.cum_width == self.cols {
                self.push_line();
            }
        }
        Ok(())
    }
}

// CR alee: what would be the syntax for user-configured parses?
pub fn parse_log_line(
    lln: usize,
    cols: usize,
    line: &str,
    filter: Option<&Regex>,
) -> Result<Option<Vec<DisplayLine>>> {
    let utf8 = |s: &[u8]| -> Result<String> { Ok(std::str::from_utf8(s)?.to_string()) };
    let mut ret = DisplayLinesBuilder::new(lln, cols);
    let parse_log_level = alt((
        map(tag("ERROR"), |_| 0),
        map(tag("WARN"), |_| 1),
        map(tag("INFO"), |_| 2),
        map(tag("DEBUG"), |_| 3),
        map(tag("TRACE"), |_| 4),
    ));
    let rem = match tuple((
        consumed(tag("[")),
        consumed(iso8601::parsers::parse_datetime),
        consumed(multispace1),
        consumed(parse_log_level),
        consumed(multispace1),
        consumed(take_until("]")),
        consumed(tag("]")),
    ))(line.as_ref())
    {
        Ok((rem, (lb, ts, w, ll, ww, target, rb))) => {
            let dt: DateTime<FixedOffset> =
                ts.1.try_into().map_err(|_| anyhow!("ts conv"))?;
            ret.ts = Some(dt.with_timezone(&Utc));
            ret.ll = Some(ll.1);
            ret.push_span(Span::noise(utf8(lb.0)?))?;
            ret.push_span(Span::timestamp(utf8(ts.0)?))?;
            ret.push_span(Span::noise(utf8(w.0)?))?;
            ret.push_span(Span::level(utf8(ll.0)?))?;
            ret.push_span(Span::noise(utf8(ww.0)?))?;
            ret.push_span(Span::target(utf8(target.0)?))?;
            ret.push_span(Span::noise(utf8(rb.0)?))?;
            std::str::from_utf8(rem)?
        }
        _ => line,
    };
    match filter {
        Some(filter) => {
            if !filter.is_match(rem) {
                // short-circuit if the line doesn't match the filter
                return Ok(None);
            }
            let mut matches = vec![];
            for m in filter.find_iter(rem) {
                matches.push((m.start(), m.end()));
            }
            let mut last = 0;
            for (start, end) in matches {
                ret.push_span(Span::text(rem[last..start].to_string()))?;
                ret.push_span(Span::text_match(rem[start..end].to_string()))?;
                last = end;
            }
            if last < rem.len() {
                ret.push_span(Span::text(rem[last..].to_string()))?;
            }
        }
        None => {
            ret.push_span(Span::text(rem.to_string()))?;
        }
    }
    Ok(Some(ret.build()))
}

#[cfg(test)]
mod test {
    use super::*;

    fn melt(lines: Vec<DisplayLine>) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.text.clone())
                    .collect::<Vec<String>>()
                    .join("")
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[test]
    fn test_parse_log_line() -> Result<()> {
        let s = "[2024-02-25T20:49:42Z TRACE s8] Petersburg, used only by the elite";
        let r = parse_log_line(0, 80, s, None)?.unwrap();
        let ts: DateTime<Utc> = "2024-02-25T20:49:42Z".parse()?;
        assert_eq!(
            r,
            vec![DisplayLine {
                lln: 0,
                ll: Some(4),
                ts: Some(ts),
                spans: vec![
                    Span::noise("[".to_string()),
                    Span::timestamp("2024-02-25T20:49:42Z".to_string()),
                    Span::noise(" ".to_string()),
                    Span::level("TRACE".to_string()),
                    Span::noise(" ".to_string()),
                    Span::target("s8".to_string()),
                    Span::noise("]".to_string()),
                    Span::text(" Petersburg, used only by the elite".to_string()),
                ],
            }]
        );
        // test soft breaks
        let r = parse_log_line(0, 100, s, None)?.unwrap();
        assert_eq!(melt(r), s);
        let r = parse_log_line(0, 40, s, None)?.unwrap();
        assert_eq!(
            melt(r),
            ["[2024-02-25T20:49:42Z TRACE s8] ", "Petersburg, used only by the elite"]
                .join("\n")
        );
        let r = parse_log_line(0, 1, s, None)?.unwrap();
        assert_eq!(
            melt(r),
            s.chars().map(|c| c.to_string()).collect::<Vec<String>>().join("\n")
        );
        // make sure it doesn't stack overflow
        for i in 1..=100 {
            parse_log_line(0, i, s, None)?;
        }
        Ok(())
    }
}
