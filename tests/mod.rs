use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;

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
