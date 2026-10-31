//! Compile an explicit authoring chain without exposing XML nodes to the runtime.
use crate::{content, loader};
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    path::Path,
    sync::Arc,
};

pub const MAX_FILES: usize = 128;
pub const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
const GROUPS: &[(&str, &[&str])] = &[
    ("scripts", &["script"]),
    (
        "assets",
        &[
            "cg",
            "image",
            "nine-patch",
            "font",
            "audio",
            "video",
            "live2d",
        ],
    ),
    ("effects", &["image-effect"]),
    ("variables", &["int", "bool", "string"]),
    ("characters", &["character"]),
    ("visuals", &["visual"]),
    ("frames", &["frame"]),
];

struct Fragment {
    file: String,
    source: Arc<str>,
    range: Range<usize>,
}
struct Origin {
    output: Range<usize>,
    fragment: Fragment,
    verbatim: bool,
}
pub struct CompiledStory {
    pub xml: String,
    pub files: Vec<String>,
    origins: Vec<Origin>,
}
impl CompiledStory {
    /// Random IDs are a publication step only; the authoring XML stays unchanged.
    pub fn publication_xml(&self) -> Result<String> {
        let doc = Document::parse(&self.xml)?;
        let nodes: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("frame"))
            .collect();
        let mut used: BTreeSet<_> = nodes
            .iter()
            .flat_map(|n| [n.attribute("id"), n.attribute("name")])
            .flatten()
            .map(str::to_owned)
            .collect();
        let mut insertions = Vec::new();
        for node in nodes {
            if node.attribute("id").is_some() {
                continue;
            }
            loader::required("story.xml", node, "name")?;
            let mut chosen = None;
            for _ in 0..8 {
                let mut bytes = [0u8; 16];
                getrandom::fill(&mut bytes).map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format("host-story-chain-0001", &[("arg0", e.to_string())])
                    )
                })?;
                bytes[6] = (bytes[6] & 15) | 64;
                bytes[8] = (bytes[8] & 63) | 128;
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                let id = format!(
                    "{}-{}-{}-{}-{}",
                    &hex[..8],
                    &hex[8..12],
                    &hex[12..16],
                    &hex[16..20],
                    &hex[20..]
                );
                if used.insert(id.clone()) {
                    chosen = Some(id);
                    break;
                }
            }
            let id = chosen.context(crate::i18n::text("host-story-chain-0011"))?;
            insertions.push((node.range().start + "<frame".len(), format!(" id=\"{id}\"")));
        }
        let mut xml = self.xml.clone();
        for (at, text) in insertions.into_iter().rev() {
            xml.insert_str(at, &text);
        }
        ensure!(
            xml.len() <= MAX_XML_BYTES,
            "{}",
            crate::i18n::format("host-story-chain-0002", &[])
        );
        Ok(xml)
    }
    fn append(&mut self, text: &str, fragment: Fragment, verbatim: bool) {
        let start = self.xml.len();
        self.xml.push_str(text);
        self.origins.push(Origin {
            output: start..self.xml.len(),
            fragment,
            verbatim,
        });
    }
    /// Translate a loader diagnostic from generated coordinates to the author's file.
    pub fn diagnostic(&self, error: anyhow::Error) -> anyhow::Error {
        let message = format!("{error:#}");
        let Some(rest) = message.strip_prefix("story.xml:") else {
            return error;
        };
        let mut fields = rest.splitn(3, ':');
        let Some(row) = fields.next().and_then(|s| s.parse::<usize>().ok()) else {
            return error;
        };
        let Some(col) = fields.next().and_then(|s| s.parse::<usize>().ok()) else {
            return error;
        };
        let Some(detail) = fields.next() else {
            return error;
        };
        let Some(offset) = offset_at(&self.xml, row, col) else {
            return error;
        };
        let Some(origin) = self.origins.iter().find(|o| o.output.contains(&offset)) else {
            return error;
        };
        let original = origin.fragment.range.start
            + if origin.verbatim {
                offset - origin.output.start
            } else {
                0
            };
        let prefix = &origin.fragment.source[..original];
        let row = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let col = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        anyhow::anyhow!("{}:{row}:{col}:{detail}", origin.fragment.file)
    }
}
fn offset_at(text: &str, row: usize, col: usize) -> Option<usize> {
    if row == 0 || col == 0 {
        return None;
    }
    let start = if row == 1 {
        0
    } else {
        text.match_indices('\n').nth(row - 2)?.0 + 1
    };
    let line = text[start..].split('\n').next()?;
    let column = line
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(line.len()))
        .nth(col - 1)?;
    Some(start + column)
}
fn fragment(file: &str, source: &Arc<str>, node: Node<'_, '_>) -> Fragment {
    Fragment {
        file: file.to_owned(),
        source: source.clone(),
        range: node.range(),
    }
}
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
/// A link is an assembly edge, never an implicit scene transition.
pub(crate) fn next(file: &str, root: Node<'_, '_>) -> Result<Option<String>> {
    let node = loader::single(file, root, "next")?;
    loader::check(file, node, "next", &["src", "end"], &[], false)?;
    match (node.attribute("src"), node.attribute("end")) {
        (Some(src), None) => {
            content::validate_path(Path::new(src)).map_err(|e| loader::error(file, node, e))?;
            if src.len() > 1024
                || Path::new(src).extension().and_then(|s| s.to_str()) != Some("xml")
            {
                return Err(loader::error(
                    file,
                    node,
                    crate::i18n::text("host-story-chain-0012"),
                ));
            }
            Ok(Some(src.to_owned()))
        }
        (None, Some("true")) => Ok(None),
        _ => Err(loader::error(
            file,
            node,
            crate::i18n::text("host-story-chain-0013"),
        )),
    }
}

pub fn compile(source: &dyn content::ContentSource) -> Result<CompiledStory> {
    let first: Arc<str> = content::text(source, Path::new("story.xml"))?.into();
    let document = Document::parse(&first).context(crate::i18n::text("host-story-chain-0014"))?;
    ensure!(
        document.root_element().attribute("schema") == Some("8"),
        "{}",
        crate::i18n::format("host-story-chain-0003", &[])
    );
    drop(document);
    let mut output = CompiledStory {
        xml: String::new(),
        files: Vec::new(),
        origins: Vec::new(),
    };
    let mut groups: BTreeMap<&str, (Fragment, Vec<Fragment>)> = BTreeMap::new();
    let mut interface = None;
    let mut localization = None;
    let mut file = "story.xml".to_owned();
    let mut edge = "story.xml:1:1".to_owned();
    let mut bytes = 0usize;
    let mut visited = BTreeSet::new();
    loop {
        ensure!(
            output.files.len() < MAX_FILES,
            "{}",
            crate::i18n::format(
                "host-story-chain-0004",
                &[
                    ("arg0", edge.to_string()),
                    ("arg1", MAX_FILES.to_string()),
                    ("arg2", file.to_string())
                ]
            )
        );
        ensure!(
            visited.insert(file.to_lowercase()),
            "{}",
            crate::i18n::format(
                "host-story-chain-0005",
                &[("arg0", edge.to_string()), ("arg1", file.to_string())]
            )
        );
        let text = if output.files.is_empty() {
            first.clone()
        } else {
            content::text(source, Path::new(&file))
                .with_context(|| {
                    crate::i18n::format(
                        "host-story-chain-0006",
                        &[("arg0", edge.to_string()), ("arg1", file.to_string())],
                    )
                })?
                .into()
        };
        bytes += text.len();
        ensure!(
            bytes <= MAX_XML_BYTES,
            "{}",
            crate::i18n::format("host-story-chain-0007", &[("arg0", file.to_string())])
        );
        let doc = Document::parse(&text).with_context(|| {
            crate::i18n::format("host-story-chain-0008", &[("arg0", file.to_string())])
        })?;
        let root = doc.root_element();
        let entry = output.files.is_empty();
        loader::check(
            &file,
            root,
            "story",
            if entry {
                &["schema", "id", "content-version", "entry"]
            } else {
                &["schema"]
            },
            if entry {
                &[
                    "interface",
                    "localization",
                    "scripts",
                    "assets",
                    "effects",
                    "variables",
                    "characters",
                    "visuals",
                    "frames",
                    "next",
                ]
            } else {
                &[
                    "scripts",
                    "assets",
                    "effects",
                    "variables",
                    "characters",
                    "visuals",
                    "frames",
                    "next",
                ]
            },
            false,
        )?;
        if loader::required(&file, root, "schema")? != "8" {
            return Err(loader::error(
                &file,
                root,
                crate::i18n::text("host-story-chain-0015"),
            ));
        }
        if entry {
            let id = escaped(loader::required(&file, root, "id")?);
            let version = escaped(loader::required(&file, root, "content-version")?);
            let start = escaped(loader::required(&file, root, "entry")?);
            output.append(&format!("<story schema=\"8\" id=\"{id}\" content-version=\"{version}\" entry=\"{start}\">\n"), fragment(&file, &text, root), false);
        }
        let link = next(&file, root)?;
        let link_position = doc.text_pos_at(loader::single(&file, root, "next")?.range().start);
        edge = format!("{file}:{}:{}", link_position.row, link_position.col);
        let mut seen = BTreeSet::new();
        for node in loader::elements(root).filter(|n| !n.has_tag_name("next")) {
            let name = node.tag_name().name();
            if !seen.insert(name) {
                return Err(loader::error(
                    &file,
                    node,
                    crate::i18n::format("host-story-chain-0009", &[("arg0", name.to_string())]),
                ));
            }
            if name == "localization" {
                localization = Some(fragment(&file, &text, node));
            } else if name == "interface" {
                interface = Some(fragment(&file, &text, node));
            } else {
                let (name, children) = GROUPS
                    .iter()
                    .find(|(group, _)| *group == name)
                    .context(crate::i18n::text("host-story-chain-0016"))?;
                loader::check(&file, node, name, &[], children, false)?;
                let (_, items) = groups
                    .entry(name)
                    .or_insert_with(|| (fragment(&file, &text, node), Vec::new()));
                items.extend(loader::elements(node).map(|n| fragment(&file, &text, n)));
            }
        }
        output.files.push(file);
        match link {
            Some(src) => file = src,
            None => break,
        }
    }
    if let Some(item) = localization {
        let raw = item.source[item.range.clone()].to_owned();
        output.append(&raw, item, true);
        output.xml.push('\n');
    }
    if let Some(item) = interface {
        let raw = item.source[item.range.clone()].to_owned();
        output.append(&raw, item, true);
        output.xml.push('\n');
    }
    for (name, _) in GROUPS {
        // The shared loader validates required groups and global references.
        if let Some((origin, items)) = groups.remove(name) {
            output.append(&format!("<{name}>\n"), origin, false);
            for item in items {
                let raw = item.source[item.range.clone()].to_owned();
                output.append(&raw, item, true);
                output.xml.push('\n');
            }
            output.xml.push_str(&format!("</{name}>\n"));
        }
    }
    output.xml.push_str("<next end=\"true\"/>\n</story>\n");
    ensure!(
        output.xml.len() <= MAX_XML_BYTES,
        "{}",
        crate::i18n::format("host-story-chain-0010", &[])
    );
    Ok(output)
}
