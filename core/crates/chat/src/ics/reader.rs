//! The reader of an `.ics` file: bytes become a tree of components and their properties, under
//! the limits a file from outside must meet. Nothing is interpreted here; a value stays text
//! until `import` reads it.

use super::{IcsError, MAX_DEPTH, MAX_LINE_BYTES};

/// A `BEGIN`…`END` block: its properties, then the blocks inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub props: Vec<Property>,
    pub children: Vec<Component>,
}

/// One content line: `NAME;PARAM=value:value`. Names are upper case, a parameter's value is
/// unquoted and several of them are joined by commas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Component {
    /// The first property with that name.
    pub fn prop(&self, name: &str) -> Option<&Property> {
        self.props.iter().find(|prop| prop.name == name)
    }
}

impl Property {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// The calendar in `bytes`: the one `VCALENDAR` the file holds, with nothing after it.
pub fn calendar(bytes: &[u8]) -> Result<Component, IcsError> {
    let text = std::str::from_utf8(bytes).map_err(|_| IcsError::Invalid)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader::default();
    for line in logical_lines(text)? {
        reader.line(&line)?;
    }
    reader.finish()
}

/// The content lines of a file, with each folded continuation joined onto its line. A physical
/// line over the limit is refused, folded or not.
fn logical_lines(text: &str) -> Result<Vec<String>, IcsError> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if raw.len() > MAX_LINE_BYTES {
            return Err(IcsError::Invalid);
        }
        match raw.strip_prefix(|ch: char| ch == ' ' || ch == '\t') {
            Some(rest) => lines.last_mut().ok_or(IcsError::Invalid)?.push_str(rest),
            None => lines.push(raw.to_string()),
        }
    }
    Ok(lines)
}

/// The components still open, and the calendar once it has closed.
#[derive(Default)]
struct Reader {
    open: Vec<Component>,
    calendar: Option<Component>,
}

impl Reader {
    fn line(&mut self, line: &str) -> Result<(), IcsError> {
        if line.trim().is_empty() {
            return Ok(());
        }
        if self.calendar.is_some() {
            return Err(IcsError::Invalid);
        }
        let property = parse_line(line)?;
        match property.name.as_str() {
            "BEGIN" => self.begin(&property.value),
            "END" => self.end(&property.value),
            _ => {
                let open = self.open.last_mut().ok_or(IcsError::Invalid)?;
                open.props.push(property);
                Ok(())
            }
        }
    }

    fn begin(&mut self, name: &str) -> Result<(), IcsError> {
        if self.open.len() == MAX_DEPTH || !valid_name(name) {
            return Err(IcsError::Invalid);
        }
        self.open.push(Component {
            name: name.to_ascii_uppercase(),
            props: Vec::new(),
            children: Vec::new(),
        });
        Ok(())
    }

    fn end(&mut self, name: &str) -> Result<(), IcsError> {
        let closed = self.open.pop().ok_or(IcsError::Invalid)?;
        if closed.name != name.to_ascii_uppercase() {
            return Err(IcsError::Invalid);
        }
        match self.open.last_mut() {
            Some(parent) => parent.children.push(closed),
            None => self.calendar = Some(closed),
        }
        Ok(())
    }

    fn finish(self) -> Result<Component, IcsError> {
        match self.calendar {
            Some(calendar) if self.open.is_empty() && calendar.name == "VCALENDAR" => Ok(calendar),
            _ => Err(IcsError::Invalid),
        }
    }
}

/// `NAME;PARAM=value:value`, the parameters' quoted values may hold `:`, `;` and `,`.
fn parse_line(line: &str) -> Result<Property, IcsError> {
    let name_end = line.find([';', ':']).ok_or(IcsError::Invalid)?;
    let name = &line[..name_end];
    if !valid_name(name) {
        return Err(IcsError::Invalid);
    }
    let mut rest = &line[name_end..];
    let mut params = Vec::new();
    while let Some(after) = rest.strip_prefix(';') {
        let (param, tail) = parameter(after)?;
        params.push(param);
        rest = tail;
    }
    let value = rest.strip_prefix(':').ok_or(IcsError::Invalid)?;
    Ok(Property {
        name: name.to_ascii_uppercase(),
        params,
        value: value.to_string(),
    })
}

/// One `PARAM=value[,value]` and the text after it.
fn parameter(text: &str) -> Result<((String, String), &str), IcsError> {
    let equals = text.find('=').ok_or(IcsError::Invalid)?;
    let name = &text[..equals];
    if !valid_name(name) {
        return Err(IcsError::Invalid);
    }
    let (value, tail) = parameter_values(&text[equals + 1..])?;
    Ok(((name.to_ascii_uppercase(), value), tail))
}

fn parameter_values(text: &str) -> Result<(String, &str), IcsError> {
    let mut values = Vec::new();
    let mut rest = text;
    loop {
        let (value, after) = match rest.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"').ok_or(IcsError::Invalid)?;
                (&quoted[..end], &quoted[end + 1..])
            }
            None => {
                let end = rest.find([',', ';', ':']).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            }
        };
        values.push(value);
        match after.strip_prefix(',') {
            Some(next) => rest = next,
            None => return Ok((values.join(","), after)),
        }
    }
}

/// A name is letters, digits and hyphens, as RFC 5545 has them.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}
