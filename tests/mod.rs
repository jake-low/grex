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
