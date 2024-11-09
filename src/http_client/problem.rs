use std::sync::LazyLock;
use scraper::{Html, Selector, ElementRef, Node};
use regex::{Regex, Captures};
use console::style;
use super::{Result, REDIRECT_RE, HttpClientError, HttpClient, IntoResult, Division};

#[derive(Debug)]
pub struct Problem {
    pub name: String,
    pub id: u64,
    /// human readable contest name
    pub contest: String,
    pub division: Division,
    /// just 1, 2, or 3
    pub problem_num: u8,
    /// ansi escape formatted description
    pub description: String,
    // sample test cases
    pub test_cases: Vec<TestCase>,
    /// all new problems use stdio, older ones use .in and .out files
    pub input: IoMode,
    pub output: IoMode
}

#[derive(Debug)]
pub struct TestCase {
    input: String,
    output: String
}

#[derive(Debug)]
pub enum IoMode {
    Stdio,
    File(String)
}

static MATH_ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\\(\w+)"#).unwrap()
});

static LATEX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\$(.*?)\$"#).unwrap()
});

static WS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\s+"#).unwrap()
});

/// helper function to parse text with re
fn parse_el_regex<'a>(el: Option<ElementRef<'a>>, re: &Regex) -> Option<Captures<'a>> {
    re.captures(el?
                .text()
                .next()?
                .trim()
    )  
}

/// parse problem HTML into ansi escaped text
fn parse_problem_description(el: ElementRef<'_>, is_pre: bool) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    for c in el.children() {
        match c.value() {
            Node::Text(text) => {
                let text = text.trim();
                // text node - add this to list if not empty
                if text.len() > 0 {
                    // handle match entities
                    let text = MATH_ENTITY_RE.replace_all(text, |caps: &Captures| {
                        match caps.get(1).unwrap().as_str() {
                            "leq" | "le" => "≤",
                            "geq" | "ge" => "≥",
                            "lt" => "<",
                            "gt" => ">",
                            "dots" => "…",
                            "cdot" => "•",
                            _ => "?"
                        }
                    });
                    // handle math formatting
                    let text = LATEX_RE.replace_all(text.as_ref(), |caps: &Captures| {
                        style(caps.get(1).unwrap().as_str()).italic().yellow().to_string()
                    });
                    if is_pre {
                        parts.push(text.into());
                    } else {
                        let mut text: String = WS_RE.replace_all(text.as_ref(), " ").into();
                        if text.starts_with("Problem credits") {
                            text = style(text).magenta().to_string();
                        }
                        parts.push(text);
                    }
                }
            },
            Node::Element(e) => {
                // convert c to an element
                let c_el = ElementRef::wrap(c).unwrap();
                if e.name() == "ul" {
                    // format like a list
                    let children = c_el.child_elements()
                        .filter_map(|e| parse_problem_description(e, false))
                        .map(|s| format!(" • {}", s))
                        .collect::<Vec<_>>()
                        .join("\n");
                    parts.push(children);
                } else if let Some(mut result) = parse_problem_description(c_el, e.name() == "pre") {
                    if e.name() == "h4" {
                        result = format!(
                            "\n{}",
                            style(format!("{}", result))
                                .bold().blue().underlined()
                                .to_string()
                        );
                    } else if e.name() == "pre" {
                        result = style(result)
                            .italic()
                            .to_string();
                    }
                    parts.push(result);
                }
            },
            _ => {}
        }
    }
    // don't return anything if empty (to save memory)
    if parts.len() == 0 { None } else { Some(parts.join("\n")) }
}

impl HttpClient {
    pub async fn get_problem(&self, problem_id: u64) -> Result<Problem> {
        let res = self.client
            .get(&format!("https://usaco.org/index.php?page=viewproblem2&cpid={}", problem_id))
            .send()
            .await?;

        let body: String = res.text().await?;
        // not found
        if REDIRECT_RE.find(&body).is_some() {
            return Err(HttpClientError::ProblemNotFound);
        }

        let doc = Html::parse_document(&body);
        let h2_selector = Selector::parse("h2").unwrap();
        let mut headings = doc.select(&h2_selector);
        // parse the first heading (contest and division)
        let h1_re =  Regex::new(r#"^USACO ([^,]+), (\w+)$"#).unwrap();
        let h1 = parse_el_regex(headings.next(), &h1_re)
            .ir_msg("could not find first heading")?;
        // parse the second heading (problem number/name)
        let h2_re = Regex::new(r#"^Problem (\d)\. (.+)$"#).unwrap();
        let h2 = parse_el_regex(headings.next(), &h2_re)
            .ir_msg("could not find second heading")?;

        // parse the input/output format
        let input_format_selector = Selector::parse(".prob-in-spec > h4").unwrap();
        let output_format_selector = Selector::parse(".prob-out-spec > h4").unwrap();
        let io_mode_re = Regex::new(r#"^(?:OUTPUT|INPUT) FORMAT \(file ([\w\.]+)\):$"#).unwrap();
        let input_format = match parse_el_regex(
            doc.select(&input_format_selector).next(),
            &io_mode_re
        ) {
            Some(cap) => IoMode::File(cap.get(1).unwrap().as_str().into()),
            None => IoMode::Stdio, // default to stdio
        };
        let output_format = match parse_el_regex(
            doc.select(&output_format_selector).next(),
            &io_mode_re
        ) {
            Some(cap) => IoMode::File(cap.get(1).unwrap().as_str().into()),
            None => IoMode::Stdio,
        };

        // parse out test cases
        let in_case_selector = Selector::parse("pre.in").unwrap();
        let out_case_selector = Selector::parse("pre.out").unwrap();
        let in_cases = doc
            .select(&in_case_selector)
            .filter_map(|a| a.text().next())
            .map(|s| s.to_string());
        let out_cases = doc
            .select(&out_case_selector)
            .filter_map(|a| a.text().next())
            .map(|s| s.to_string());

        // combine both iterators into one
        let test_cases = in_cases
            .zip(out_cases)
            .map(|(input, output)| TestCase { input, output })
            .collect();

        let description_selector = Selector::parse("#probtext-text").unwrap();
        let description = doc
            .select(&description_selector)
            .next()
            .ir_msg("could not find problem description")?;
        let description = parse_problem_description(description, false).unwrap_or_else(|| "".into());
        
        Ok(Problem {
            id: problem_id,
            name: h2.get(2).ir_msg("could not parse name")?.as_str().into(),
            contest: h1.get(1).ir_msg("could not parse contest")?.as_str().into(),
            division: h1.get(2).and_then(|s| Division::from_str(s.as_str())).ir_msg("could not parse division")?,
            problem_num: h2.get(1).and_then(|s| s.as_str().parse().ok()).ir_msg("could not parse problem num")?,
            input: input_format,
            output: output_format,
            test_cases,
            description
        })
    }
}
