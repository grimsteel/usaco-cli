use std::sync::LazyLock;
use scraper::{Html, Selector, ElementRef};
use regex::{Regex, Captures};
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

static H1_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^USACO ([^,]+), (\w+)$"#).unwrap()
});

static H2_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^Problem (\d)\. (.+)$"#).unwrap()
});

static IO_MODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:OUTPUT|INPUT) FORMAT \(file ([\w\.]+)\):$"#).unwrap()
});

// helper function to parse text with re
fn parse_el_regex<'a>(el: Option<ElementRef<'a>>, re: &Regex) -> Option<Captures<'a>> {
    re.captures(el?
                .text()
                .next()?
                .trim()
    )  
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
        let h1 = parse_el_regex(headings.next(), &H1_RE)
            .ir_msg("could not find first heading")?;
        // parse the second heading (problem number/name)
        let h2 = parse_el_regex(headings.next(), &H2_RE)
            .ir_msg("could not find second heading")?;

        // parse the input/output format
        let input_format_selector = Selector::parse(".prob-in-spec > h4").unwrap();
        let output_format_selector = Selector::parse(".prob-out-spec > h4").unwrap();
        let input_format = match parse_el_regex(
            doc.select(&input_format_selector).next(),
            &IO_MODE_RE
        ) {
            Some(cap) => IoMode::File(cap.get(1).unwrap().as_str().into()),
            None => IoMode::Stdio, // default to stdio
        };
        let output_format = match parse_el_regex(
            doc.select(&output_format_selector).next(),
            &IO_MODE_RE
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
        
        Ok(Problem {
            id: problem_id,
            name: h2.get(2).ir_msg("could not parse name")?.as_str().into(),
            contest: h1.get(1).ir_msg("could not parse contest")?.as_str().into(),
            division: h1.get(2).and_then(|s| Division::from_str(s.as_str())).ir_msg("could not parse division")?,
            problem_num: h2.get(1).and_then(|s| s.as_str().parse().ok()).ir_msg("could not parse problem num")?,
            input: input_format,
            output: output_format,
            test_cases,
            description: "".into()
        })
    }
}
