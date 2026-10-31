//! Bounded UI authoring assembly. The runtime only receives owned, validated data.
use crate::{content, loader};
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    ops::Range,
    path::Path,
    sync::Arc,
};

pub const MAX_FILES: usize = 128;
pub const MAX_DEPTH: usize = 16;
pub const MAX_XML_BYTES: usize = 1024 * 1024;
const ATTRS: &[&str] = &[
    "schema",
    "width",
    "height",
    "initial",
    "title",
    "title-bind",
    "title-key",
    "title-key-bind",
];

#[derive(Clone)]
struct Fragment {
    file: String,
    text: Arc<str>,
    range: Range<usize>,
}
impl Fragment {
    fn at(file: &str, text: &Arc<str>, node: Node<'_, '_>) -> Self {
        Self {
            file: file.into(),
            text: text.clone(),
            range: node.range(),
        }
    }
}
struct Origin {
    output: Range<usize>,
    fragment: Fragment,
    verbatim: bool,
}
pub struct CompiledUi {
    pub entry: String,
    pub xml: String,
    pub files: Vec<String>,
    origins: Vec<Origin>,
}
impl CompiledUi {
    fn append(&mut self, text: &str, fragment: Fragment, verbatim: bool) -> Result<()> {
        ensure!(
            self.xml.len() + text.len() <= MAX_XML_BYTES,
            "{}",
            crate::i18n::format(
                "host-interface-source-0001",
                &[("arg0", self.entry.to_string())]
            )
        );
        let start = self.xml.len();
        self.xml.push_str(text);
        self.origins.push(Origin {
            output: start..self.xml.len(),
            fragment,
            verbatim,
        });
        Ok(())
    }
    fn remap(&self, message: &str) -> String {
        let Some(rest) = message.strip_prefix(&format!("{}:", self.entry)) else {
            return message.into();
        };
        let Some((row, tail)) = rest.split_once(':') else {
            return message.into();
        };
        let end = tail
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(tail.len());
        let position = (|| {
            let row = row.parse::<usize>().ok()?.checked_sub(1)?;
            let col = tail[..end].parse::<usize>().ok()?.checked_sub(1)?;
            let start = if row == 0 {
                0
            } else {
                self.xml.match_indices('\n').nth(row - 1)?.0 + 1
            };
            let line = self.xml[start..].split('\n').next()?;
            let offset = start
                + line
                    .char_indices()
                    .map(|(at, _)| at)
                    .chain(std::iter::once(line.len()))
                    .nth(col)?;
            let origin = self.origins.iter().find(|o| o.output.contains(&offset))?;
            let original = origin.fragment.range.start
                + if origin.verbatim {
                    offset - origin.output.start
                } else {
                    0
                };
            let prefix = &origin.fragment.text[..original];
            let row = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
            let col = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            Some(format!(
                "{}:{row}:{col}{}",
                origin.fragment.file,
                &tail[end..]
            ))
        })();
        position.unwrap_or_else(|| message.into())
    }
    pub fn parse(
        &self,
        story: &crate::model::Story,
        registrations: &[super::Registration],
    ) -> Result<super::Definition> {
        self.parse_localized(
            story,
            registrations,
            std::sync::Arc::new(crate::localization::Catalog::default()),
        )
    }
    pub fn parse_localized(
        &self,
        story: &crate::model::Story,
        registrations: &[super::Registration],
        catalog: crate::localization::SharedCatalog,
    ) -> Result<super::Definition> {
        let mut definition =
            super::parse_localized(&self.xml, &self.entry, story, registrations, catalog)
                .map_err(|e| anyhow::anyhow!(self.remap(&format!("{e:#}"))))?;
        definition.window_title.location = self.remap(&definition.window_title.location);
        if let Some(binding) = &mut definition.localized_title {
            binding.location = self.remap(&binding.location);
        }
        fn nodes(compiled: &CompiledUi, items: &mut [super::Node]) {
            for node in items {
                node.location = compiled.remap(&node.location);
                if let Some(binding) = &mut node.localized {
                    binding.location = compiled.remap(&binding.location);
                }
                nodes(compiled, &mut node.children);
            }
        }
        for screen in definition.screens.values_mut() {
            screen.location = self.remap(&screen.location);
            nodes(self, &mut screen.nodes);
        }
        Ok(definition)
    }
}

struct Assembly<'a> {
    source: &'a dyn content::ContentSource,
    output: CompiledUi,
    seen: BTreeSet<String>,
    bytes: usize,
    root: Option<(Fragment, String)>,
    groups: BTreeMap<String, (Fragment, Vec<Fragment>)>,
    screens: Vec<Fragment>,
}
fn path(file: &str, node: Node<'_, '_>, name: &str) -> Result<String> {
    content::validate_path(Path::new(name)).map_err(|e| loader::error(file, node, e))?;
    if name.len() > 1024 || Path::new(name).extension().and_then(|s| s.to_str()) != Some("xml") {
        return Err(loader::error(
            file,
            node,
            crate::i18n::text("host-interface-source-0012"),
        ));
    }
    Ok(name.into())
}
impl Assembly<'_> {
    fn visit(&mut self, file: String, depth: usize, edge: String) -> Result<()> {
        ensure!(
            depth <= MAX_DEPTH,
            "{}",
            crate::i18n::format(
                "host-interface-source-0002",
                &[("arg0", edge.to_string()), ("arg1", file.to_string())]
            )
        );
        ensure!(
            self.output.files.len() < MAX_FILES,
            "{}",
            crate::i18n::format("host-interface-source-0003", &[("arg0", edge.to_string())])
        );
        ensure!(
            self.seen.insert(file.to_lowercase()),
            "{}",
            crate::i18n::format(
                "host-interface-source-0004",
                &[("arg0", edge.to_string()), ("arg1", file.to_string())]
            )
        );
        let mut bytes = Vec::new();
        self.source
            .open(Path::new(&file))
            .with_context(|| {
                crate::i18n::format(
                    "host-interface-source-0005",
                    &[("arg0", edge.to_string()), ("arg1", file.to_string())],
                )
            })?
            .take((MAX_XML_BYTES - self.bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| {
                crate::i18n::format("host-interface-source-0006", &[("arg0", file.to_string())])
            })?;
        self.bytes += bytes.len();
        ensure!(
            self.bytes <= MAX_XML_BYTES,
            "{}",
            crate::i18n::format("host-interface-source-0007", &[("arg0", file.to_string())])
        );
        let text: Arc<str> = String::from_utf8(bytes)
            .with_context(|| {
                crate::i18n::format("host-interface-source-0008", &[("arg0", file.to_string())])
            })?
            .into();
        let document = Document::parse(&text).map_err(|e| {
            let pos = e.pos();
            anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "host-interface-source-0009",
                    &[
                        ("arg0", file.to_string()),
                        ("arg1", pos.row.to_string()),
                        ("arg2", pos.col.to_string()),
                        ("arg3", e.to_string())
                    ]
                )
            )
        })?;
        let root = document.root_element();
        loader::check(
            &file,
            root,
            "ui",
            if depth == 1 { ATTRS } else { &["schema"] },
            &[
                "variables",
                "gallery",
                "palette",
                "screen",
                "include",
                "ui-media",
            ],
            false,
        )?;
        if loader::required(&file, root, "schema")? != "3" {
            return Err(loader::error(
                &file,
                root,
                crate::i18n::text("host-interface-source-0013"),
            ));
        }
        if depth == 1 {
            let mut header = String::from("<ui");
            for attr in root.attributes() {
                header.push_str(&format!(" {}=\"{}\"", attr.name(), escaped(attr.value())));
            }
            header.push_str(">\n");
            self.root = Some((Fragment::at(&file, &text, root), header));
        }
        self.output.files.push(file.clone());
        let mut groups = BTreeSet::new();
        for node in loader::elements(root) {
            match node.tag_name().name() {
                "include" => {
                    loader::check(&file, node, "include", &["src"], &[], false)?;
                    let next = path(&file, node, loader::required(&file, node, "src")?)?;
                    let pos = document.text_pos_at(node.range().start);
                    self.visit(next, depth + 1, format!("{file}:{}:{}", pos.row, pos.col))?;
                }
                "screen" => self.screens.push(Fragment::at(&file, &text, node)),
                group => {
                    if !groups.insert(group) {
                        return Err(loader::error(
                            &file,
                            node,
                            crate::i18n::format(
                                "host-interface-source-0010",
                                &[("arg0", group.to_string())],
                            ),
                        ));
                    }
                    loader::check(
                        &file,
                        node,
                        group,
                        &[],
                        if group == "variables" {
                            &["int", "bool", "string"]
                        } else if group == "ui-media" {
                            &["sound", "sequence"]
                        } else if group == "palette" {
                            &["color"]
                        } else {
                            &["cg"]
                        },
                        false,
                    )?;
                    let (_, items) = self
                        .groups
                        .entry(group.into())
                        .or_insert_with(|| (Fragment::at(&file, &text, node), Vec::new()));
                    items.extend(loader::elements(node).map(|n| Fragment::at(&file, &text, n)));
                }
            }
        }
        Ok(())
    }
}
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}
pub fn compile(source: &dyn content::ContentSource, entry: &Path) -> Result<CompiledUi> {
    content::validate_path(entry)?;
    let entry = entry
        .to_str()
        .context(crate::i18n::text("host-interface-source-0014"))?;
    ensure!(
        entry.len() <= 1024 && Path::new(entry).extension().and_then(|s| s.to_str()) == Some("xml"),
        "{}",
        crate::i18n::format("host-interface-source-0011", &[])
    );
    let mut assembly = Assembly {
        source,
        output: CompiledUi {
            entry: entry.into(),
            xml: String::new(),
            files: Vec::new(),
            origins: Vec::new(),
        },
        seen: BTreeSet::new(),
        bytes: 0,
        root: None,
        groups: BTreeMap::new(),
        screens: Vec::new(),
    };
    assembly.visit(entry.into(), 1, format!("{entry}:1:1"))?;
    let (root, header) = assembly
        .root
        .context(crate::i18n::text("host-interface-source-0015"))?;
    let mut output = assembly.output;
    if output.files.len() == 1 {
        // Keep the current single-file contract byte-for-byte, including comments.
        let full = Fragment {
            range: 0..root.text.len(),
            ..root
        };
        output.append(&full.text, full.clone(), true)?;
        return Ok(output);
    }
    output.append(&header, root.clone(), false)?;
    for (group, (origin, items)) in assembly.groups {
        output.append(&format!("<{group}>\n"), origin.clone(), false)?;
        for item in items {
            output.append(&item.text[item.range.clone()], item.clone(), true)?;
            output.append("\n", origin.clone(), false)?;
        }
        output.append(&format!("</{group}>\n"), origin, false)?;
    }
    for item in assembly.screens {
        output.append(&item.text[item.range.clone()], item.clone(), true)?;
        output.append("\n", root.clone(), false)?;
    }
    output.append("</ui>\n", root, false)?;
    Ok(output)
}
