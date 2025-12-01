use super::{Division, HttpClient, HttpClientError, IntoResult, Result, REDIRECT_RE};
use console::style;
use regex::{Captures, Regex};
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    sync::LazyLock,
};
use zip::ZipArchive;

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    /// data released after the competition ends
    pub released_data: Option<ReleasedProblemData>,
    /// sample test cases
    pub test_cases: Vec<TestCase>,
    /// all new problems use stdio, older ones use .in and .out files
    pub input: IoMode,
    pub output: IoMode,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TestCase {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum IoMode {
    Stdio,
    File(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleasedProblemData {
    /// ansi escape formatted writeup
    pub writeup: String,
    /// writeup URL
    pub writeup_url: String,
    /// official test case data
    pub official_test_case_url: String,
}

// these regexes are re-used
static MATHCAL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\\mathcal\{([A-Z])\}"#).unwrap());
static MATH_ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\\(\w+)"#).unwrap());
static LATEX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\$(.*?)\$"#).unwrap());
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\s+"#).unwrap());

/// helper function to parse text with re
fn parse_el_regex<'a>(el: Option<ElementRef<'a>>, re: &Regex) -> Option<Captures<'a>> {
    re.captures(el?.text().next()?.trim())
}

/// parse problem HTML into ansi escaped text
fn parse_problem_description(el: ElementRef<'_>, is_pre: bool, is_inline: bool) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    for c in el.children() {
        match c.value() {
            Node::Text(text) => {
                let text = text.trim();
                // text node - add this to list if not empty
                if text.len() > 0 {
                    // generally used for Big-O notation
                    let text = MATHCAL_RE.replace_all(text, |caps: &Captures| {
                        // just passthrough
                        caps.get(1).unwrap().as_str().to_string()
                    });
                    // handle match entities
                    let text = MATH_ENTITY_RE.replace_all(text.as_ref(), |caps: &Captures| {
                        match caps.get(1).unwrap().as_str() {
                            "leq" | "le" => "≤",
                            "geq" | "ge" => "≥",
                            "lt" => "<",
                            "gt" => ">",
                            "dots" | "ldots" => "…",
                            "cdot" => "•",
                            _ => "?",
                        }
                    });
                    // handle math formatting
                    let text = LATEX_RE.replace_all(text.as_ref(), |caps: &Captures| {
                        style(caps.get(1).unwrap().as_str())
                            .italic()
                            .yellow()
                            .to_string()
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
            }
            Node::Element(e) => {
                // convert c to an element
                let c_el = ElementRef::wrap(c).unwrap();
                if e.name() == "script" {
                    continue;
                } else if e.name() == "ul" {
                    // format like a list
                    let children = c_el
                        .child_elements()
                        .filter_map(|e| parse_problem_description(e, false, false))
                        .map(|s| format!(" • {}", s))
                        .collect::<Vec<_>>()
                        .join("\n");
                    parts.push(children);
                } else if let Some(mut result) = parse_problem_description(
                    c_el,
                    e.name() == "pre",
                    e.name() == "p" || e.name() == "strong",
                ) {
                    if e.name() == "h4" || e.name() == "strong" {
                        result = format!(
                            "\n{}",
                            style(format!("{}", result))
                                .bold()
                                .blue()
                                .underlined()
                                .to_string()
                        );
                    } else if e.name() == "pre" {
                        result = style(result).italic().color256(255).to_string();
                    }
                    parts.push(result);
                }
            }
            _ => {}
        }
    }
    // don't return anything if empty (to save memory)
    if parts.len() == 0 {
        None
    } else {
        Some(parts.join(if is_inline { " " } else { "\n" }))
    }
}

impl HttpClient {
    /// Fetch released test case and writeup data for a problem
    async fn get_released_problem_data(
        &self,
        problem_id: u64,
        doc: &Html,
    ) -> Option<ReleasedProblemData> {
        // get the problem list url
        let button_selector = Selector::parse("button").unwrap();
        let button = doc.select(&button_selector).next()?;
        let location_re = Regex::new(r#"window\.location='([^']+)';"#).unwrap();
        let problem_list_url = format!(
            "https://usaco.org/{}",
            location_re
                .captures(button.attr("onclick")?)?
                .get(1)
                .unwrap()
                .as_str()
        );

        // fetch the problem list doc
        let res = self.client.get(problem_list_url).send().await.ok()?;

        let body: String = res.text().await.ok()?;
        let pl_doc = Html::parse_document(&body);

        // figure out where this problem is on the problem list
        let problem_link_selector = Selector::parse(&format!(
            r#"a[href="index.php?page=viewproblem2&cpid={}"]"#,
            problem_id
        ))
        .unwrap();

        let problem_link = pl_doc.select(&problem_link_selector).next()?;

        let mut link_siblings = problem_link.next_siblings().filter_map(|node| {
            let el = ElementRef::wrap(node)?;
            if el.value().name() == "a" {
                // make absolute
                Some(format!("https://usaco.org/{}", el.attr("href")?))
            } else {
                None
            }
        });

        let test_data_url = link_siblings.next()?.to_string();
        let writeup_url = link_siblings.next()?.to_string();

        // fetch the writeup
        let writeup_res = self.client.get(&writeup_url).send().await.ok()?;

        // parse the writeup
        let writeup_body: String = writeup_res.text().await.ok()?;
        let body_selector = Selector::parse("body").unwrap();
        let writeup_doc = Html::parse_document(&writeup_body);
        let writeup =
            parse_problem_description(writeup_doc.select(&body_selector).next()?, false, false)
                .unwrap_or_default();

        Some(ReleasedProblemData {
            official_test_case_url: test_data_url,
            writeup_url,
            writeup,
        })
    }

    /// download official test cases from zip file and parse
    pub async fn get_official_test_cases(&self, zip_url: &str) -> Result<Vec<TestCase>> {
        let res = self.client.get(zip_url).send().await?;
        let body = Cursor::new(res.bytes().await?);
        let mut zip = ZipArchive::new(body)?;

        // old format == {I,O}.[0-9]
        // new format = [0-9].{in,out}
        let mut old_format = false;
        let mut num_cases: u8 = 0;

        // figure out how many test cases there are and what format they use
        for file in zip.file_names() {
            if let Some((name, ext)) = file.split_once('.') {
                if let Ok(num) = if name == "I" || name == "O" {
                    old_format = true;
                    // number in extension
                    ext.parse()
                } else {
                    // number in filename
                    name.parse()
                } {
                    // update num cases
                    if num > num_cases {
                        num_cases = num;
                    }
                }
            } else {
                // error out
                return Err(HttpClientError::UnexpectedResponse(
                    "Unknown test case file name format",
                ));
            }
        }

        let mut vec = vec![];
        for case_id in 1..=num_cases {
            // filenames of in/out files
            let (in_name, out_name) = if old_format {
                (format!("I.{}", case_id), format!("O.{}", case_id))
            } else {
                (format!("{}.in", case_id), format!("{}.out", case_id))
            };

            let in_contents = zip.by_name(&in_name).ok().and_then(|mut file| {
                let mut contents = String::new();
                file.read_to_string(&mut contents).ok()?;
                Some(contents)
            });
            let out_contents = zip.by_name(&out_name).ok().and_then(|mut file| {
                let mut contents = String::new();
                file.read_to_string(&mut contents).ok()?;
                Some(contents)
            });

            // read in/out files
            if let Some((input, output)) = in_contents.zip(out_contents) {
                vec.push(TestCase { input, output })
            }
        }

        Ok(vec)
    }

    /// Parse a `Problem` out of a problem view HTML document 
    pub async fn parse_problem_html(&self, problem_id: u64, problem_body: String, fetch_released_data: bool) -> Result<Problem> {
        let doc = Html::parse_document(&problem_body);
        let h2_selector = Selector::parse("h2").unwrap();
        let mut headings = doc.select(&h2_selector);
        // parse the first heading (contest and division)
        let h1_re = Regex::new(r#"^USACO ([^,]+), (\w+)$"#).unwrap();
        let h1 = parse_el_regex(headings.next(), &h1_re).ir_msg("could not find first heading")?;
        // parse the second heading (problem number/name)
        let h2_re = Regex::new(r#"^Problem (\d)\. (.+)$"#).unwrap();
        let h2 = parse_el_regex(headings.next(), &h2_re).ir_msg("could not find second heading")?;

        // parse the input/output format
        let input_format_selector = Selector::parse(".prob-in-spec > h4").unwrap();
        let output_format_selector = Selector::parse(".prob-out-spec > h4").unwrap();
        let io_mode_re = Regex::new(r#"^(?:OUTPUT|INPUT) FORMAT \(file ([\w\.]+)\):$"#).unwrap();
        let input_format =
            match parse_el_regex(doc.select(&input_format_selector).next(), &io_mode_re) {
                Some(cap) => IoMode::File(cap.get(1).unwrap().as_str().into()),
                None => IoMode::Stdio, // default to stdio
            };
        let output_format =
            match parse_el_regex(doc.select(&output_format_selector).next(), &io_mode_re) {
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
        let description =
            parse_problem_description(description, false, false).unwrap_or_else(|| "".into());
        
        // only fetch released data if needed
        let released_data = if fetch_released_data {
            self.get_released_problem_data(problem_id, &doc).await
        } else {
            None
        };

        // construct problem struct
        Ok(Problem {
            id: problem_id,
            name: h2.get(2).ir_msg("could not parse name")?.as_str().into(),
            contest: h1.get(1).ir_msg("could not parse contest")?.as_str().into(),
            division: h1
                .get(2)
                .and_then(|s| Division::from_str(s.as_str()))
                .ir_msg("could not parse division")?,
            problem_num: h2
                .get(1)
                .and_then(|s| s.as_str().parse().ok())
                .ir_msg("could not parse problem num")?,
            input: input_format,
            output: output_format,
            test_cases,
            description,
            released_data,
        })
    }

    /// Fetch a problem with the given ID
    pub async fn get_problem(&self, problem_id: u64) -> Result<Problem> {
        let res = self
            .client
            .get(&format!(
                "https://usaco.org/index.php?page=viewproblem2&cpid={}",
                problem_id
            ))
            .send()
            .await?;

        let body: String = res.text().await?;
        // not found
        if REDIRECT_RE.find(&body).is_some() {
            return Err(HttpClientError::ProblemNotFound);
        }

        self.parse_problem_html(problem_id, body, true).await
    }
}
