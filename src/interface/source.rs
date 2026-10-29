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
            "{}: 合并 UI XML 超过 1MiB",
            self.entry
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
        handlers: &BTreeSet<String>,
    ) -> Result<super::Definition> {
        let mut definition = super::parse(&self.xml, &self.entry, story, handlers)
            .map_err(|e| anyhow::anyhow!(self.remap(&format!("{e:#}"))))?;
        definition.window_title.location = self.remap(&definition.window_title.location);
        fn nodes(compiled: &CompiledUi, items: &mut [super::Node]) {
            for node in items {
                node.location = compiled.remap(&node.location);
                nodes(compiled, &mut node.children);
            }
        }
        for screen in definition.screens.values_mut() {
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
            "UI src 必须是至多 1024 字节的游戏内 .xml 路径",
        ));
    }
    Ok(name.into())
}
impl Assembly<'_> {
    fn visit(&mut self, file: String, depth: usize, edge: String) -> Result<()> {
        ensure!(depth <= MAX_DEPTH, "{edge}: UI 引入超过 16 层（{file}）");
        ensure!(
            self.output.files.len() < MAX_FILES,
            "{edge}: UI 引入超过 128 文件"
        );
        ensure!(
            self.seen.insert(file.to_lowercase()),
            "{edge}: UI 循环或重复/大小写重复引入：{file}"
        );
        let mut bytes = Vec::new();
        self.source
            .open(Path::new(&file))
            .with_context(|| format!("{edge}: 读取 UI {file}"))?
            .take((MAX_XML_BYTES - self.bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .with_context(|| format!("{file}: 读取 UI XML"))?;
        self.bytes += bytes.len();
        ensure!(self.bytes <= MAX_XML_BYTES, "{file}: UI XML 累计超过 1MiB");
        let text: Arc<str> = String::from_utf8(bytes)
            .with_context(|| format!("{file} 不是 UTF-8"))?
            .into();
        let document = Document::parse(&text).map_err(|e| {
            let pos = e.pos();
            anyhow::anyhow!("{file}:{}:{}: UI XML 解析失败：{e}", pos.row, pos.col)
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
            return Err(loader::error(&file, root, "仅支持 UI schema 3"));
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
                        return Err(loader::error(&file, node, format!("重复 <{group}>")));
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
    let entry = entry.to_str().context("UI 路径必须 UTF-8")?;
    ensure!(
        entry.len() <= 1024 && Path::new(entry).extension().and_then(|s| s.to_str()) == Some("xml"),
        "UI 入口必须为游戏内 .xml 路径"
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
    let (root, header) = assembly.root.context("UI 入口")?;
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
