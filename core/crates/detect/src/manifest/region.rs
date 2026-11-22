//! Screen regions: which part of the tail a rule reads.
//! Adapted from herdr (Apache-2.0) `src/detect/manifest.rs` (`region`).

#[derive(Debug)]
pub(super) enum Region {
    WholeRecent,
    OscTitle,
    AfterLastHorizontalRule,
    BottomNonEmptyLines(usize),
    BottomLines(usize),
    TopNonEmptyLines(usize),
}

pub(super) fn parse_region(spec: &str) -> Option<Region> {
    let spec = spec.trim();
    match spec {
        "whole_recent" => return Some(Region::WholeRecent),
        "osc_title" => return Some(Region::OscTitle),
        "after_last_horizontal_rule" => return Some(Region::AfterLastHorizontalRule),
        _ => {}
    }
    if let Some(count) = region_count(spec, "bottom_non_empty_lines") {
        return Some(Region::BottomNonEmptyLines(count));
    }
    if let Some(count) = region_count(spec, "bottom_lines") {
        return Some(Region::BottomLines(count));
    }
    if let Some(count) = region_count(spec, "top_non_empty_lines") {
        return Some(Region::TopNonEmptyLines(count));
    }
    None
}

fn region_count(spec: &str, name: &str) -> Option<usize> {
    spec.strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')?
        .parse()
        .ok()
}

pub(super) fn region_text(region: &Region, screen_tail: &str, osc_title: &str) -> String {
    match region {
        Region::WholeRecent => screen_tail.to_string(),
        Region::OscTitle => osc_title.to_string(),
        Region::AfterLastHorizontalRule => after_last_horizontal_rule(screen_tail).to_string(),
        Region::BottomNonEmptyLines(count) => {
            bottom_non_empty_lines(screen_tail, *count).join("\n")
        }
        Region::BottomLines(count) => bottom_lines(screen_tail, *count).join("\n"),
        Region::TopNonEmptyLines(count) => top_non_empty_lines(screen_tail, *count).join("\n"),
    }
}

/// Everything after the last line that is a horizontal rule (three or more
/// `─`). herdr uses this to isolate a live dialog from older output.
fn after_last_horizontal_rule(text: &str) -> &str {
    let mut offset = 0;
    let mut last_rule_end = 0;
    for line in text.split_inclusive('\n') {
        offset += line.len();
        let trimmed = line.trim();
        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '─') {
            last_rule_end = offset;
        }
    }
    &text[last_rule_end..]
}

fn bottom_lines(text: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].to_vec()
}

fn bottom_non_empty_lines(text: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].to_vec()
}

fn top_non_empty_lines(text: &str, count: usize) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .take(count)
        .collect()
}
