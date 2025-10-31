use std::collections::HashMap;
use std::ffi::CString;
use std::io::{self, BufWriter, Read, Write};

use anyhow::{Context, Result};
use clap::Parser;
use libxml::bindings::{xmlKeepBlanksDefault, xmlSaveFormatFileEnc};
use libxml::parser::{Parser as XmlParser, ParserOptions};
use libxml::tree::node::set_node_rc_guard;
use libxml::tree::{Document, Node};

#[derive(Parser)]
#[command(version, about)]
struct CliArgs {
    #[arg(long)]
    ungrex: bool,
    input_file: Option<String>,
}

fn main() -> Result<()> {
    sigpipe::reset();
    let args = CliArgs::parse();

    if args.ungrex {
        ungrex_mode(&args)?;
    } else {
        grex_mode(&args)?;
    }
    Ok(())
}

fn grex_mode(args: &CliArgs) -> Result<()> {
    let input = read_input(args)?;

    if input.trim().is_empty() {
        return Ok(());
    }

    let parser = XmlParser::default();
    let options = ParserOptions {
        recover: false,
        ..Default::default()
    };
    let doc = parser
        .parse_string_with_options(&input, options)
        .context("Failed to parse XML")?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    if let Some(root) = doc.get_root_element() {
        traverse_node(&root, None, None, &mut writer)?;
    }
    writer.flush()?;
    Ok(())
}

fn traverse_node(
    node: &Node,
    parent_path: Option<&str>,
    sibling_index: Option<usize>,
    writer: &mut impl Write,
) -> Result<()> {
    let name = if let Some(index) = sibling_index {
        format!("{}[{}]", node.get_name(), index)
    } else {
        node.get_name().to_string()
    };

    let path = if let Some(parent_path) = parent_path {
        format!("{}/{}", parent_path, name)
    } else {
        format!("/{}", name)
    };

    let mut attrs: Vec<_> = node.get_properties().into_iter().collect();
    // sort attrs alphabetically
    // TODO: does libxml2 preserve attribute order after parsing? might be nice
    // to output attrs in the same order they're given in the input
    attrs.sort_by(|a, b| a.0.cmp(&b.0));
    for (attr_name, attr_value) in attrs {
        writeln!(
            writer,
            "{}/@{} = {}",
            path,
            attr_name,
            escape_value(&attr_value)
        )?;
    }

    let has_element_children = node.get_first_element_child().is_some();

    if !has_element_children {
        let text = get_text_content(node);
        if !text.trim().is_empty() {
            writeln!(writer, "{}/text() = {}", path, escape_value(&text))?;
        }
    }

    let children: Vec<Node> = node.get_child_elements();
    let mut child_counts: HashMap<String, usize> = HashMap::new();
    for child in &children {
        *child_counts.entry(child.get_name()).or_insert(0) += 1;
    }

    let mut child_indices: HashMap<String, usize> = HashMap::new();

    for child in children {
        let child_name = child.get_name();
        let idx = child_indices
            .entry(child_name.clone())
            .and_modify(|e| *e += 1)
            .or_insert(1);

        let total = child_counts[&child_name];
        let sibling_index = (total > 1).then_some(*idx);

        traverse_node(&child, Some(&path), sibling_index, writer)?;
    }
    Ok(())
}

fn get_text_content(node: &Node) -> String {
    node.get_child_nodes()
        .into_iter()
        .filter(|child| child.get_type() == Some(libxml::tree::NodeType::TextNode))
        .map(|child| child.get_content())
        .collect()
}

fn escape_value(s: &str) -> String {
    s.chars()
        .flat_map(|ch| match ch {
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\t' => vec!['\\', 't'],
            _ => vec![ch],
        })
        .collect()
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

fn escape_xml_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

fn ungrex_mode(args: &CliArgs) -> Result<()> {
    let input = read_input(args)?;

    // Increase rc_guard to allow caching nodes while still mutating them;
    // default is 2, but we need one extra for the prefix cache
    set_node_rc_guard(3);

    // Treat whitespace as not significant for pretty printing
    unsafe {
        xmlKeepBlanksDefault(0);
    }

    let mut doc = Document::new().expect("Failed to create document");

    // cache maps xpath strings to nodes in the tree, so that we don't
    // have to keep reevaluating the same xpath expressions.
    // TODO: use a LRU cache or something to bound memory usage
    // (or maybe just cache the ancestors of the current node?)
    let mut cache: HashMap<String, Node> = HashMap::new();

    for (line_num, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let (xpath, value) = parse_grex_line(line)
            .with_context(|| format!("line {}: malformed grex input", line_num + 1))?;
        apply_grex_line(&mut doc, &xpath, &value, &mut cache)
            .with_context(|| format!("line {}", line_num + 1))?;
    }

    print_document(&doc);
    Ok(())
}

fn print_document(doc: &Document) {
    unsafe {
        let filename = CString::new("-").unwrap();
        let encoding = CString::new("UTF-8").unwrap();
        let format = 1;

        xmlSaveFormatFileEnc(filename.as_ptr(), doc.doc_ptr(), encoding.as_ptr(), format);
    }
}

fn parse_grex_line(line: &str) -> Option<(String, String)> {
    let (xpath, value) = line.split_once(" = ")?;
    Some((xpath.to_string(), unescape_value(value)))
}

fn apply_grex_line(
    doc: &mut Document,
    xpath: &str,
    value: &str,
    cache: &mut HashMap<String, Node>,
) -> Result<()> {
    if let Some(element_path) = xpath.strip_suffix("/text()") {
        let mut node = get_or_create_node(doc, element_path, cache)?;

        let escaped = escape_xml_entities(value);
        node.set_content(&escaped)
            .map_err(|e| anyhow::anyhow!("Failed to set text content: {:?}", e))?;
        Ok(())
    } else if let Some(attr_pos) = xpath.rfind("/@") {
        let element_path = &xpath[..attr_pos];
        let attr_name = &xpath[attr_pos + 2..];
        let mut node = get_or_create_node(doc, element_path, cache)?;
        node.set_attribute(attr_name, value)
            .map_err(|e| anyhow::anyhow!("Failed to set attribute: {:?}", e))?;
        Ok(())
    } else {
        anyhow::bail!("Invalid XPath: must end with /text() or /@attr")
    }
}

fn get_or_create_node(
    doc: &mut Document,
    path: &str,
    cache: &mut HashMap<String, Node>,
) -> Result<Node> {
    if let Some(node) = cache.get(path) {
        return Ok(node.clone());
    }

    let parts = parse_xpath(path)?;

    if doc.get_root_element().is_none() {
        if parts.is_empty() {
            anyhow::bail!("Empty path");
        }
        let root_name = &parts[0].name;
        let root = Node::new(root_name, None, doc)
            .map_err(|e| anyhow::anyhow!("Failed to create root: {:?}", e))?;
        doc.set_root_element(&root);
    }

    let mut current = doc.get_root_element().expect("root element should exist");
    let mut current_path = format!("/{}", parts[0].name);

    cache.insert(current_path.clone(), current.clone());

    // iterate over path components and find or create the corresponding elements
    for part in &parts[1..] {
        current_path.push('/');
        current_path.push_str(&part.name);
        if let Some(idx) = part.index {
            current_path.push_str(&format!("[{}]", idx));
        }

        if let Some(cached_node) = cache.get(&current_path) {
            current = cached_node.clone();
        } else {
            current = get_or_create_child(&mut current, &part.name, part.index, doc)?;
            cache.insert(current_path.clone(), current.clone());
        }
    }

    Ok(current)
}

struct PathPart {
    name: String,
    index: Option<usize>,
}

fn parse_xpath(path: &str) -> Result<Vec<PathPart>> {
    if !path.starts_with('/') {
        anyhow::bail!("XPath must start with '/': {}", path);
    }

    let path = &path[1..];
    if path.is_empty() {
        return Ok(vec![]);
    }

    path.split('/')
        .map(|part| {
            if let Some(bracket_pos) = part.find('[') {
                let name = part[..bracket_pos].to_string();
                let index_str = &part[bracket_pos + 1..part.len() - 1];
                let index = Some(
                    index_str
                        .parse::<usize>()
                        .with_context(|| format!("Invalid index in path component: {}", part))?,
                );
                Ok(PathPart { name, index })
            } else {
                Ok(PathPart {
                    name: part.to_string(),
                    index: None,
                })
            }
        })
        .collect()
}

fn get_or_create_child(
    parent: &mut Node,
    name: &str,
    index: Option<usize>,
    doc: &Document,
) -> Result<Node> {
    let mut matching_children: Vec<_> = parent
        .get_child_elements()
        .into_iter()
        .filter(|child| child.get_name() == name)
        .collect();

    let count = matching_children.len();

    if let Some(idx) = index {
        // find child with given index, or create it (and any preceeding siblings)
        // if it doesn't
        if idx <= count {
            return Ok(matching_children[idx - 1].clone());
        }
        for _ in count..idx {
            let mut child = Node::new(name, None, doc)
                .map_err(|_| anyhow::anyhow!("Failed to create child"))?;
            parent
                .add_child(&mut child)
                .map_err(|e| anyhow::anyhow!("Failed to add child node: {}", e))?;
            matching_children.push(child);
        }
        Ok(matching_children.last().unwrap().clone())
    } else {
        // no index specified; return first or create
        if let Some(child) = matching_children.first() {
            Ok(child.clone())
        } else {
            let mut child = Node::new(name, None, doc)
                .map_err(|_| anyhow::anyhow!("Failed to create child"))?;
            parent
                .add_child(&mut child)
                .map_err(|e| anyhow::anyhow!("Failed to add child node: {}", e))?;
            Ok(child)
        }
    }
}

fn read_input(args: &CliArgs) -> Result<String> {
    if let Some(filename) = &args.input_file {
        Ok(std::fs::read_to_string(filename)?)
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    }
}
