use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use libxml::parser::Parser as XmlParser;
use libxml::xpath::Context;

fn grex() -> Command {
    cargo_bin_cmd!()
}

fn run_grex(input_file: &str) -> String {
    String::from_utf8(
        grex()
            .arg(input_file)
            .output()
            .expect("Failed to run grex")
            .stdout,
    )
    .expect("Invalid UTF-8")
}

fn run_grex_stdin(input: &str) -> String {
    String::from_utf8(
        grex()
            .write_stdin(input)
            .output()
            .expect("Failed to run grex")
            .stdout,
    )
    .expect("Invalid UTF-8")
}

fn run_ungrex(input: &str) -> String {
    String::from_utf8(
        grex()
            .arg("--ungrex")
            .write_stdin(input)
            .output()
            .expect("Failed to run ungrex")
            .stdout,
    )
    .expect("Invalid UTF-8")
}

// Macro to generate grex_* and ungrex_* tests for each valid test pair
macro_rules! generate_tests {
    ($($name:ident),* $(,)?) => {
        $(
            paste::paste! {
                #[test]
                fn [<grex_ $name>]() {
                    let output = run_grex(concat!("tests/valid/", stringify!($name), ".xml"));
                    let expected = fs::read_to_string(concat!("tests/valid/", stringify!($name), ".grex"))
                        .expect("Failed to read expected grex file");
                    assert_eq!(output, expected);
                }

                #[test]
                fn [<ungrex_ $name>]() {
                    let grex_input = fs::read_to_string(concat!("tests/valid/", stringify!($name), ".grex"))
                        .expect("Failed to read grex file");
                    let xml_output = run_ungrex(&grex_input);

                    // NOTE: the direct thing to do would be to compare the output XML with
                    // the test XML file, but that's tricky, since two XML documents may
                    // represent equivalent trees without being byte-for-byte identical
                    // (due to whitespace, order of attributes, etc).
                    //
                    // As a workaround, we re-grex the output XML and compare it to the input.
                    // This works at the moment, but could break if the Grex encoding details
                    // change (e.g. outputting attributes in order they're found in the input
                    // rather than sorted order). A better solution would be to parse the
                    // expected and actual XML and walk both trees to compare them.
                    let roundtrip_grex = run_grex_stdin(&xml_output);
                    assert_eq!(grex_input, roundtrip_grex);
                }
            }
        )*
    };
}

generate_tests!(
    simple,
    pizzeria,
    attrs,
    empty_elements,
    special_chars,
    unicode,
    deep_nesting,
    many_siblings,
    whitespace,
);

#[test]
fn test_malformed_xml() {
    grex().arg("tests/invalid/malformed.xml").assert().failure();
}

#[test]
fn test_invalid_grex_missing_separator() {
    grex()
        .arg("--ungrex")
        .arg("tests/invalid/missing_separator.grex")
        .assert()
        .failure();
}

#[test]
fn test_invalid_grex_bad_xpath() {
    grex()
        .arg("--ungrex")
        .arg("tests/invalid/bad_xpath.grex")
        .assert()
        .failure();
}

#[test]
fn test_empty_xml_input() {
    grex().write_stdin("").assert().success().stdout("");
}

#[test]
fn test_empty_grex_input() {
    let output = grex()
        .arg("--ungrex")
        .write_stdin("")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).expect("Invalid UTF-8");
    assert!(output.contains("<?xml"));
}

#[test]
fn test_nonexistent_file() {
    grex().arg("nonexistent-file.xml").assert().failure();
}

#[test]
fn workflow_grep_filter_attributes() {
    let grex_output = run_grex("tests/valid/pizzeria.xml");

    let filtered: Vec<&str> = grex_output
        .lines()
        .filter(|line| line.contains("@price"))
        .collect();

    assert_eq!(filtered.len(), 3);
    assert!(filtered[0].contains("14.99"));
    assert!(filtered[1].contains("15.99"));
    assert!(filtered[2].contains("17.99"));
}

#[test]
fn workflow_grep_filter_path() {
    let grex_output = run_grex("tests/valid/pizzeria.xml");

    let filtered: Vec<&str> = grex_output
        .lines()
        .filter(|line| line.contains("/location/"))
        .collect();

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().any(|line| line.contains("address")));
    assert!(filtered.iter().any(|line| line.contains("phone")));
}

#[test]
fn workflow_grep_filter_value() {
    let grex_output = run_grex("tests/valid/pizzeria.xml");

    let filtered: Vec<&str> = grex_output
        .lines()
        .filter(|line| line.contains("Anchovy"))
        .collect();

    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].contains("/pizzeria/menu/pizza[3]/@name"));
}

#[test]
fn workflow_filter_and_reconstruct() {
    let grex_output = run_grex("tests/valid/pizzeria.xml");

    let filtered: String = grex_output
        .lines()
        .filter(|line| !line.contains("/menu/pizza"))
        .collect::<Vec<&str>>()
        .join("\n");

    let reconstructed = run_ungrex(&filtered);

    assert!(reconstructed.contains("<?xml"));
    assert!(reconstructed.contains("<pizzeria"));
    assert!(reconstructed.contains("<location>"));

    assert!(!reconstructed.contains("<pizza"));
}

#[test]
fn workflow_sed_style_value_replacement() {
    let grex_output = run_grex("tests/valid/pizzeria.xml");

    let modified = grex_output.replace("Panucci's Pizza", "Joe's Pizza");

    let reconstructed = run_ungrex(&modified);

    assert!(reconstructed.contains("Joe's Pizza"));
    assert!(!reconstructed.contains("Panucci's Pizza"));
}

// tests that grex output is stable after XML -> grex -> XML -> grex
#[test]
fn property_roundtrip_grex_stable() {
    for xml_path in find_valid_test_files() {
        let xml_path_str = xml_path.to_str().unwrap();

        let grex_output = run_grex(xml_path_str);
        let xml_output = run_ungrex(&grex_output);
        let roundtrip_grex = run_grex_stdin(&xml_output);

        let original_lines: Vec<&str> = grex_output.lines().collect();
        let roundtrip_lines: Vec<&str> = roundtrip_grex.lines().collect();

        assert_eq!(
            original_lines,
            roundtrip_lines,
            "Grex output not stable after roundtrip for {}",
            xml_path.file_name().unwrap().to_str().unwrap()
        );
    }
}

// tests that XML output is stable after grex -> XML -> grex -> XML
#[test]
fn property_roundtrip_xml_stable() {
    for xml_path in find_valid_test_files() {
        let xml_path_str = xml_path.to_str().unwrap();

        let grex_output = run_grex(xml_path_str);
        let xml1 = run_ungrex(&grex_output);
        let grex2 = run_grex_stdin(&xml1);
        let xml2 = run_ungrex(&grex2);

        assert_eq!(
            xml1,
            xml2,
            "XML output not stable after roundtrip for {}",
            xml_path.file_name().unwrap().to_str().unwrap()
        );
    }
}

// tests that every Xpath selector in the grex output is valid
#[test]
fn property_xpath_selectors_are_correct() {
    for xml_path in find_valid_test_files() {
        let xml_path_str = xml_path.to_str().unwrap();
        let grex_path = xml_path.with_extension("grex");

        let xml_content = fs::read_to_string(xml_path_str).expect("Failed to read XML file");
        let grex_content = fs::read_to_string(&grex_path).expect("Failed to read grex file");

        let parser = XmlParser::default();
        let doc = parser
            .parse_string(&xml_content)
            .expect("Failed to parse XML");
        let context = Context::new(&doc).expect("Failed to create XPath context");

        for (line_num, line) in grex_content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let (xpath, expected_value) = parse_grex_line(line)
                .unwrap_or_else(|| panic!("Failed to parse grex line {}: {}", line_num + 1, line));

            let actual_value = evaluate_xpath(&context, &xpath).unwrap_or_else(|e| {
                panic!(
                    "Failed to evaluate XPath '{}' on line {}: {}",
                    xpath,
                    line_num + 1,
                    e
                )
            });

            assert_eq!(
                actual_value,
                expected_value,
                "XPath '{}' on line {} of {} returned wrong value",
                xpath,
                line_num + 1,
                xml_path.file_name().unwrap().to_str().unwrap()
            );
        }
    }
}

fn find_valid_test_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = glob::glob("tests/valid/*.xml")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|path| path.with_extension("grex").exists())
        .collect();
    files.sort();
    assert!(!files.is_empty(), "No test files found");
    files
}

fn evaluate_xpath(context: &Context, xpath: &str) -> Result<String, String> {
    let result = context
        .evaluate(xpath)
        .map_err(|e| format!("Failed to evaluate XPath: {:?}", e))?;

    let nodes = result.get_nodes_as_vec();
    if nodes.is_empty() {
        return Err("XPath returned no nodes".to_string());
    }

    if nodes.len() > 1 {
        return Err(format!("XPath returned {} nodes, expected 1", nodes.len()));
    }

    let node = &nodes[0];

    if let Some(attr_value) = node.get_property("") {
        // attribute node
        Ok(attr_value)
    } else {
        // text node
        Ok(node.get_content())
    }
}

// HACK: these two functions are copy-pasted from src/main.rs
fn parse_grex_line(line: &str) -> Option<(String, String)> {
    let (xpath, value) = line.split_once(" = ")?;
    Some((xpath.to_string(), unescape_value(value)))
}

fn unescape_value(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some(other) => {
                    // invalid escape sequence; keep the backslash and character
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'), // trailing backslash
            }
        } else {
            result.push(ch);
        }
    }
    result
}
