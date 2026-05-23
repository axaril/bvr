mod state;
mod widget;

use crate::{colors, regex_compile};
use bvr_core::{LineSet, SegBuffer};
use ratatui::style::Color;
use regex::bytes::Regex;
use std::str::FromStr;

pub use state::Compositor as State;
pub use widget::FilterViewerWidget as Widget;

#[derive(Clone)]
enum FilterSet {
    All,
    Bookmarks(Bookmarks),
    Search(LineSet),
}

#[derive(Clone)]
pub enum Mask {
    All,
    Bookmarks,
    Regex(Regex),
}

impl Mask {
    pub fn build(pattern: &str, literal: bool) -> Result<(Self, Regex), regex::Error> {
        let regex = if literal {
            regex_compile(&regex::escape(pattern))
        } else {
            regex_compile(pattern)
        }?;
        Ok((Self::Regex(regex.clone()), regex))
    }

    pub fn regex(&self) -> Option<Regex> {
        match self {
            Self::All | Self::Bookmarks => None,
            Self::Regex(regex) => Some(regex.clone()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Mask::All => "All Lines",
            Mask::Bookmarks => "Bookmarks",
            Mask::Regex(regex) => regex.as_str(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct FilterExportSet {
    name: Option<String>,
    filters: Vec<FilterExport>,
}

impl FilterExportSet {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn filters(&self) -> &[FilterExport] {
        self.filters.as_ref()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum MaskExport {
    All,
    Bookmarks,
    #[serde(rename = "regex")]
    Regex {
        regex: String,
    },
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct FilterExport {
    mask: MaskExport,
    enabled: bool,
    color: String,
}
impl FilterExport {
    pub fn color(&self) -> Color {
        ratatui::style::Color::from_str(&self.color).unwrap_or_default()
    }

    pub fn name(&self) -> &str {
        match &self.mask {
            MaskExport::All => "All",
            MaskExport::Bookmarks => "Bookmarks",
            MaskExport::Regex { regex } => regex.as_str(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone)]
pub struct Filter {
    mask: Mask,
    enabled: bool,
    color: Color,
    data: FilterSet,
}

impl Filter {
    fn all() -> Self {
        Self {
            mask: Mask::All,
            data: FilterSet::All,
            enabled: true,
            color: Color::White,
        }
    }

    fn bookmark() -> Self {
        Self {
            mask: Mask::Bookmarks,
            enabled: true,
            color: colors::SELECT_ACCENT,
            data: FilterSet::Bookmarks(Bookmarks::new()),
        }
    }

    fn new(filter: Mask, color: Color, repr: FilterSet) -> Self {
        Self {
            mask: filter,
            enabled: true,
            color,
            data: repr,
        }
    }

    pub fn to_export(&self) -> FilterExport {
        FilterExport {
            mask: match &self.mask {
                Mask::All => MaskExport::All,
                Mask::Bookmarks => MaskExport::Bookmarks,
                Mask::Regex(regex) => MaskExport::Regex {
                    regex: regex.to_string(),
                },
            },
            enabled: self.enabled,
            color: self.color.to_string().to_ascii_lowercase(),
        }
    }

    pub fn from_export(file: &SegBuffer, export: &FilterExport) -> Self {
        let mask = match export.mask {
            MaskExport::All | MaskExport::Bookmarks => {
                unreachable!("should have been processed before")
            }
            MaskExport::Regex { ref regex } => Mask::Regex(regex_compile(regex).unwrap()),
        };
        Self {
            data: FilterSet::Search(LineSet::search(
                file.segment_iter().unwrap(),
                mask.regex().unwrap(),
            )),
            mask,
            enabled: export.enabled,
            color: ratatui::style::Color::from_str(&export.color).unwrap_or_default(),
        }
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn mask(&self) -> &Mask {
        &self.mask
    }

    pub fn has_line(&self, line_number: usize) -> bool {
        match &self.data {
            FilterSet::All => true,
            FilterSet::Bookmarks(lines) => lines.has_line(line_number),
            FilterSet::Search(lines) => lines.has_line(line_number),
        }
    }

    pub fn len(&self) -> Option<usize> {
        match &self.data {
            FilterSet::All => None,
            FilterSet::Bookmarks(lines) => Some(lines.len()),
            FilterSet::Search(lines) => Some(lines.len()),
        }
    }

    pub fn as_line_matches(&self) -> LineSet {
        match &self.data {
            FilterSet::All => LineSet::empty(),
            FilterSet::Bookmarks(mask) => mask.lines.clone().into(),
            FilterSet::Search(mask) => mask.clone(),
        }
    }

    pub fn nearest_forward(&self, line_number: usize) -> Option<usize> {
        match &self.data {
            FilterSet::All => None,
            FilterSet::Bookmarks(mask) => mask.nearest_forward(line_number),
            FilterSet::Search(mask) => mask.nearest_forward(line_number),
        }
    }

    pub fn nearest_backward(&self, line_number: usize) -> Option<usize> {
        match &self.data {
            FilterSet::All => None,
            FilterSet::Bookmarks(mask) => mask.nearest_backward(line_number),
            FilterSet::Search(mask) => mask.nearest_backward(line_number),
        }
    }

    pub fn is_bookmark(&self) -> bool {
        matches!(&self.data, FilterSet::Bookmarks(_))
    }

    pub fn is_complete(&self) -> bool {
        match &self.data {
            FilterSet::All => true,
            FilterSet::Bookmarks(_) => true,
            FilterSet::Search(lines) => lines.is_complete(),
        }
    }

    pub(crate) fn set_color(&mut self, color: Color) {
        match &self.data {
            FilterSet::All | FilterSet::Bookmarks(_) => (),
            FilterSet::Search(_) => self.color = color,
        }
    }
}

#[derive(Clone)]
pub struct Bookmarks {
    lines: Vec<usize>,
}

impl Bookmarks {
    fn new() -> Bookmarks {
        Bookmarks { lines: Vec::new() }
    }

    pub fn add(&mut self, line_number: usize) {
        if let Err(idx) = self.lines.binary_search(&line_number) {
            self.lines.insert(idx, line_number);
        }
    }

    pub fn remove(&mut self, line_number: usize) {
        if let Ok(idx) = self.lines.binary_search(&line_number) {
            self.lines.remove(idx);
        }
    }

    pub fn toggle(&mut self, line_number: usize) {
        match self.lines.binary_search(&line_number) {
            Ok(idx) => {
                self.lines.remove(idx);
            }
            Err(idx) => {
                self.lines.insert(idx, line_number);
            }
        };
    }

    pub fn has_line(&self, line_number: usize) -> bool {
        let slice = self.lines.as_slice();
        match *slice {
            [first, .., last] if (first..=last).contains(&line_number) => {
                slice.binary_search(&line_number).is_ok()
            }
            [item] => item == line_number,
            _ => false,
        }
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn nearest_forward(&self, line_number: usize) -> Option<usize> {
        let slice = self.lines.as_slice();
        match *slice {
            [_, ..] => Some(
                slice[match slice.binary_search(&line_number) {
                    Ok(i) => i.saturating_add(1),
                    Err(i) => i,
                }
                .min(slice.len() - 1)],
            ),
            [] => None,
        }
    }

    pub fn nearest_backward(&self, line_number: usize) -> Option<usize> {
        let slice = self.lines.as_slice();
        match *slice {
            [_, ..] => Some(
                slice[match slice.binary_search(&line_number) {
                    Ok(i) | Err(i) => i,
                }
                .saturating_sub(1)
                .min(slice.len() - 1)],
            ),
            [] => None,
        }
    }

    fn clear(&mut self) {
        self.lines.clear()
    }
}

#[derive(Clone)]
pub struct Filters {
    all: Filter,
    bookmarks: Filter,
    user_filters: Vec<Filter>,
}

impl Filters {
    fn new() -> Self {
        Self {
            all: Filter::all(),
            bookmarks: Filter::bookmark(),
            user_filters: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.user_filters.len() + 2
    }

    pub fn iter(&self) -> impl Iterator<Item = &Filter> {
        std::iter::once(&self.all)
            .chain(std::iter::once(&self.bookmarks))
            .chain(self.user_filters.iter())
    }

    pub fn iter_active(&self) -> impl Iterator<Item = &Filter> {
        self.iter().filter(|filter| filter.is_enabled())
    }

    pub fn get(&self, index: usize) -> Option<&Filter> {
        match index {
            0 => Some(&self.all),
            1 => Some(&self.bookmarks),
            _ => self.user_filters.get(index - 2),
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Filter> {
        match index {
            0 => Some(&mut self.all),
            1 => Some(&mut self.bookmarks),
            _ => self.user_filters.get_mut(index - 2),
        }
    }

    pub fn bookmarks(&self) -> &Bookmarks {
        // Safety: by construction
        match &self.bookmarks.data {
            FilterSet::Bookmarks(bookmarks) => bookmarks,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn bookmarks_mut(&mut self) -> &mut Bookmarks {
        // Safety: by construction
        match &mut self.bookmarks.data {
            FilterSet::Bookmarks(bookmarks) => bookmarks,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn export(&self, name: Option<String>) -> FilterExportSet {
        FilterExportSet {
            name,
            filters: self.iter().map(Filter::to_export).collect(),
        }
    }

    pub fn import_user_filters(&mut self, file: &SegBuffer, import: &FilterExportSet) {
        self.user_filters.clear();

        for filter in import.filters.iter() {
            // Special handling for All and Bookmarks, we want to just inherit their enablement state
            match filter.mask {
                MaskExport::All => self.all.enabled = filter.enabled,
                MaskExport::Bookmarks => self.bookmarks.enabled = filter.enabled,
                MaskExport::Regex { .. } => {
                    self.user_filters.push(Filter::from_export(file, filter))
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.bookmarks_mut().clear();
        self.user_filters.clear();
    }
}
