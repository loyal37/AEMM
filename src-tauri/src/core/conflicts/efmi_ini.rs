use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use async_trait::async_trait;

use crate::{
    core::{
        conflicts::{AnalyzerOutput, ConflictAnalysisSubject, ConflictAnalyzer, conflict_id},
        deployment::{EFMI_DIRECT_STRATEGY_ID, verify_deployment_marker},
        mods::path_is_link_or_reparse_point,
    },
    errors::AppError,
    models::{Conflict, ConflictEvidence, ConflictKind, ConflictParticipant, ConflictSeverity},
    utils::validate_relative_path,
};

pub const EFMI_INI_ANALYZER_ID: &str = "efmi.ini.v1";
const MAX_INI_FILES_PER_MOD: usize = 256;
const MAX_INI_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_INI_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct EfmiIniConflictAnalyzer;

#[derive(Debug, Clone)]
struct FactOccurrence {
    subject_index: usize,
    source_path: PathBuf,
    section: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverrideKind {
    Texture,
    Shader,
}

#[derive(Debug)]
struct IniSection {
    name: String,
    assignments: Vec<(String, String)>,
}

#[derive(Debug)]
struct ParsedIni {
    namespace: Option<String>,
    sections: Vec<IniSection>,
    warnings: Vec<String>,
}

#[async_trait]
impl ConflictAnalyzer for EfmiIniConflictAnalyzer {
    fn analyzer_id(&self) -> &'static str {
        EFMI_INI_ANALYZER_ID
    }

    async fn analyze(
        &self,
        subjects: &[ConflictAnalysisSubject],
    ) -> Result<AnalyzerOutput, AppError> {
        let subjects = subjects.to_vec();
        tokio::task::spawn_blocking(move || analyze_sync(&subjects)).await?
    }
}

fn analyze_sync(subjects: &[ConflictAnalysisSubject]) -> Result<AnalyzerOutput, AppError> {
    let mut namespaces: BTreeMap<String, Vec<FactOccurrence>> = BTreeMap::new();
    let mut texture_overrides: BTreeMap<String, Vec<FactOccurrence>> = BTreeMap::new();
    let mut shader_overrides: BTreeMap<String, Vec<FactOccurrence>> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut analyzed_ini_files = 0_u64;
    let mut total_ini_bytes = 0_u64;

    'subjects: for (subject_index, subject) in subjects.iter().enumerate() {
        let active_root = match active_deployment_root(subject) {
            Ok(root) => root,
            Err(error) => {
                warnings.push(format!(
                    "{}：无法验证部署目录，已跳过 EFMI INI 分析：{error}",
                    subject.mod_name
                ));
                continue;
            }
        };
        let ini_entries = subject
            .manifest
            .entries
            .iter()
            .filter(|entry| has_ini_extension(&entry.destination_relative))
            .collect::<Vec<_>>();
        if ini_entries.len() > MAX_INI_FILES_PER_MOD {
            warnings.push(format!(
                "{}：包含 {} 个 INI，超过每个模组 {} 个的分析上限，仅分析前 {} 个。",
                subject.mod_name,
                ini_entries.len(),
                MAX_INI_FILES_PER_MOD,
                MAX_INI_FILES_PER_MOD
            ));
        }

        for entry in ini_entries.into_iter().take(MAX_INI_FILES_PER_MOD) {
            let path = match resolve_manifest_file(&active_root, &entry.destination_relative) {
                Ok(path) => path,
                Err(error) => {
                    warnings.push(format!(
                        "{} / {}：部署文件不再安全可读，已跳过：{error}",
                        subject.mod_name,
                        storage_path(&entry.source_relative)
                    ));
                    continue;
                }
            };
            let size = match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => metadata.len(),
                Ok(_) => {
                    warnings.push(format!(
                        "{} / {}：部署条目不是普通文件，已跳过。",
                        subject.mod_name,
                        storage_path(&entry.source_relative)
                    ));
                    continue;
                }
                Err(source) => {
                    warnings.push(format!(
                        "{} / {}：无法读取文件信息，已跳过：{source}",
                        subject.mod_name,
                        storage_path(&entry.source_relative)
                    ));
                    continue;
                }
            };
            if size > MAX_INI_FILE_BYTES {
                warnings.push(format!(
                    "{} / {}：INI 大小超过 4 MiB 分析上限，已跳过。",
                    subject.mod_name,
                    storage_path(&entry.source_relative)
                ));
                continue;
            }
            let next_total = total_ini_bytes
                .checked_add(size)
                .ok_or_else(|| AppError::Conflict("INI 分析字节计数超过支持范围。".to_owned()))?;
            if next_total > MAX_TOTAL_INI_BYTES {
                warnings.push("本次 EFMI INI 总量超过 64 MiB，剩余文件已跳过。".to_owned());
                break 'subjects;
            }
            total_ini_bytes = next_total;

            let bytes = match read_bounded(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    warnings.push(format!(
                        "{} / {}：无法安全读取 INI，已跳过：{error}",
                        subject.mod_name,
                        storage_path(&entry.source_relative)
                    ));
                    continue;
                }
            };
            let (text, lossy) = decode_ini(&bytes);
            if lossy {
                warnings.push(format!(
                    "{} / {}：文本编码包含无效字符，ASCII 配置键仍已尽力分析。",
                    subject.mod_name,
                    storage_path(&entry.source_relative)
                ));
            }
            let parsed = parse_ini(&text);
            analyzed_ini_files = analyzed_ini_files
                .checked_add(1)
                .ok_or_else(|| AppError::Conflict("INI 文件计数超过支持范围。".to_owned()))?;
            for warning in parsed.warnings {
                warnings.push(format!(
                    "{} / {}：{warning}",
                    subject.mod_name,
                    storage_path(&entry.source_relative)
                ));
            }
            if let Some(namespace) = parsed.namespace {
                let normalized = normalize_namespace(&namespace);
                if !normalized.is_empty() {
                    namespaces
                        .entry(normalized)
                        .or_default()
                        .push(FactOccurrence {
                            subject_index,
                            source_path: entry.source_relative.clone(),
                            section: None,
                            detail: format!("显式 namespace = {namespace}"),
                        });
                }
            }

            let resource_files = local_resource_files(&parsed.sections);
            for section in parsed.sections {
                let Some(kind) = override_kind(&section.name) else {
                    continue;
                };
                let details = override_details(&section, &resource_files);
                for hash in section
                    .assignments
                    .iter()
                    .filter(|(key, _)| key == "hash")
                    .filter_map(|(_, value)| normalize_hash(value))
                {
                    let occurrence = FactOccurrence {
                        subject_index,
                        source_path: entry.source_relative.clone(),
                        section: Some(section.name.clone()),
                        detail: format!("hash={hash}{details}"),
                    };
                    match kind {
                        OverrideKind::Texture => {
                            texture_overrides.entry(hash).or_default().push(occurrence)
                        }
                        OverrideKind::Shader => {
                            shader_overrides.entry(hash).or_default().push(occurrence)
                        }
                    }
                }
            }
        }
    }

    let mut conflicts = Vec::new();
    append_fact_conflicts(
        subjects,
        namespaces,
        ConflictKind::EfmiNamespace,
        ConflictSeverity::Error,
        "namespace",
        "多个已启用模组声明了同一显式 EFMI/3DMigoto namespace。",
        &mut conflicts,
    )?;
    append_fact_conflicts(
        subjects,
        texture_overrides,
        ConflictKind::EfmiTextureOverride,
        ConflictSeverity::Warning,
        "texture-hash",
        "多个已启用模组可能匹配同一 TextureOverride 资源 Hash。",
        &mut conflicts,
    )?;
    append_fact_conflicts(
        subjects,
        shader_overrides,
        ConflictKind::EfmiShaderOverride,
        ConflictSeverity::Warning,
        "shader-hash",
        "多个已启用模组可能匹配同一 ShaderOverride Hash。",
        &mut conflicts,
    )?;

    Ok(AnalyzerOutput {
        conflicts,
        analyzed_ini_files,
        warnings,
    })
}

fn append_fact_conflicts(
    subjects: &[ConflictAnalysisSubject],
    facts: BTreeMap<String, Vec<FactOccurrence>>,
    kind: ConflictKind,
    severity: ConflictSeverity,
    key_prefix: &str,
    summary: &str,
    conflicts: &mut Vec<Conflict>,
) -> Result<(), AppError> {
    for (fact_key, occurrences) in facts {
        let mut by_subject: BTreeMap<usize, Vec<FactOccurrence>> = BTreeMap::new();
        for occurrence in occurrences {
            by_subject
                .entry(occurrence.subject_index)
                .or_default()
                .push(occurrence);
        }
        if by_subject.len() < 2 {
            continue;
        }
        let mut participants = by_subject
            .into_iter()
            .map(|(index, mut items)| {
                let subject = subjects
                    .get(index)
                    .ok_or_else(|| AppError::DataIntegrity("EFMI 冲突索引无效。".to_owned()))?;
                items.sort_by(|left, right| {
                    storage_path(&left.source_path)
                        .cmp(&storage_path(&right.source_path))
                        .then_with(|| left.section.cmp(&right.section))
                        .then_with(|| left.detail.cmp(&right.detail))
                });
                items.dedup_by(|left, right| {
                    left.source_path == right.source_path
                        && left.section == right.section
                        && left.detail == right.detail
                });
                Ok(ConflictParticipant {
                    mod_id: subject.mod_id(),
                    mod_name: subject.mod_name.clone(),
                    load_order: subject.load_order,
                    evidence: items
                        .into_iter()
                        .map(|item| ConflictEvidence {
                            source_path: item.source_path,
                            section: item.section,
                            detail: item.detail,
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        participants.sort_by(|left, right| {
            left.load_order
                .cmp(&right.load_order)
                .then_with(|| left.mod_name.cmp(&right.mod_name))
                .then_with(|| left.mod_id.cmp(&right.mod_id))
        });
        let resource_key = format!("{key_prefix}:{fact_key}");
        let id = conflict_id(
            EFMI_INI_ANALYZER_ID,
            kind,
            &resource_key,
            participants.iter().map(|participant| participant.mod_id),
        );
        conflicts.push(Conflict {
            id,
            analyzer_id: EFMI_INI_ANALYZER_ID.to_owned(),
            kind,
            severity,
            resource_key,
            summary: summary.to_owned(),
            participants,
            winning_mod_id: None,
        });
    }
    Ok(())
}

fn active_deployment_root(subject: &ConflictAnalysisSubject) -> Result<PathBuf, AppError> {
    if subject.manifest.strategy_id == EFMI_DIRECT_STRATEGY_ID {
        if path_is_link_or_reparse_point(&subject.manifest.destination_root)?
            || path_is_link_or_reparse_point(&subject.manifest.destination_directory)?
        {
            return Err(AppError::UnsafePath(
                "EFMI Mods 或模组目录是链接或重解析点。".to_owned(),
            ));
        }
        let mods_root = fs::canonicalize(&subject.manifest.destination_root)
            .map_err(|source| AppError::file_system(&subject.manifest.destination_root, source))?;
        let canonical =
            fs::canonicalize(&subject.manifest.destination_directory).map_err(|source| {
                AppError::file_system(&subject.manifest.destination_directory, source)
            })?;
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::UnsafePath("EFMI 模组目录名称无效。".to_owned()))?;
        if !canonical.is_dir()
            || canonical.parent() != Some(mods_root.as_path())
            || name
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DISABLED"))
        {
            return Err(AppError::UnsafePath(
                "启用模组不是 EFMI Mods 中的安全直属目录。".to_owned(),
            ));
        }
        return Ok(canonical);
    }

    validate_relative_path(&subject.manifest.destination_directory)?;
    if path_is_link_or_reparse_point(&subject.manifest.destination_root)? {
        return Err(AppError::UnsafePath(
            "部署根目录是链接或重解析点。".to_owned(),
        ));
    }
    let root = subject
        .manifest
        .destination_root
        .join(&subject.manifest.destination_directory);
    if path_is_link_or_reparse_point(&root)? {
        return Err(AppError::UnsafePath(
            "模组部署目录是链接或重解析点。".to_owned(),
        ));
    }
    let canonical =
        fs::canonicalize(&root).map_err(|source| AppError::file_system(&root, source))?;
    if !canonical.is_dir() {
        return Err(AppError::UnsafePath("模组部署根路径不是目录。".to_owned()));
    }
    verify_deployment_marker(&canonical, &subject.manifest)?;
    Ok(canonical)
}

fn resolve_manifest_file(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    validate_relative_path(relative)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if path_is_link_or_reparse_point(&current)? {
            return Err(AppError::UnsafePath(format!(
                "部署 INI 包含链接或重解析点：{}",
                storage_path(relative)
            )));
        }
    }
    let canonical =
        fs::canonicalize(&current).map_err(|source| AppError::file_system(&current, source))?;
    if canonical == root || !canonical.starts_with(root) || !canonical.is_file() {
        return Err(AppError::UnsafePath(
            "部署 INI 解析到模组部署目录之外或不是普通文件。".to_owned(),
        ));
    }
    Ok(canonical)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, AppError> {
    let file = File::open(path).map_err(|source| AppError::file_system(path, source))?;
    let limit = MAX_INI_FILE_BYTES
        .checked_add(1)
        .ok_or_else(|| AppError::Conflict("INI 读取上限超过支持范围。".to_owned()))?;
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|source| AppError::file_system(path, source))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_INI_FILE_BYTES {
        return Err(AppError::Conflict(
            "INI 在读取期间增长并超过 4 MiB 上限。".to_owned(),
        ));
    }
    Ok(bytes)
}

fn decode_ini(bytes: &[u8]) -> (String, bool) {
    if let Some(body) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(body);
    }
    if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(body, true);
    }
    if let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(body, false);
    }
    decode_utf8(bytes)
}

fn decode_utf8(bytes: &[u8]) -> (String, bool) {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => (text, false),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> (String, bool) {
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            let pair = [chunk[0], chunk[1]];
            if little_endian {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            }
        })
        .collect::<Vec<_>>();
    let odd_byte = !bytes.chunks_exact(2).remainder().is_empty();
    match String::from_utf16(&units) {
        Ok(text) if !odd_byte => (text, false),
        Ok(text) => (text, true),
        Err(_) => (String::from_utf16_lossy(&units), true),
    }
}

fn parse_ini(text: &str) -> ParsedIni {
    let mut namespace = None;
    let mut sections = Vec::new();
    let mut current: Option<IniSection> = None;
    let mut warnings = Vec::new();

    for (line_index, raw_line) in text.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                if let Some(section) = current.take() {
                    sections.push(section);
                }
                let name = line[1..end].trim();
                if name.is_empty() {
                    warnings.push(format!("第 {} 行包含空节名。", line_index + 1));
                    continue;
                }
                current = Some(IniSection {
                    name: name.to_owned(),
                    assignments: Vec::new(),
                });
            } else {
                warnings.push(format!("第 {} 行的节头缺少 ]。", line_index + 1));
            }
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let value = raw_value.trim().to_owned();
        if key.is_empty() {
            continue;
        }
        if let Some(section) = current.as_mut() {
            section.assignments.push((key, value));
        } else if key == "namespace" && namespace.replace(value).is_some() {
            warnings.push(format!("第 {} 行重复声明 namespace。", line_index + 1));
        }
    }
    if let Some(section) = current {
        sections.push(section);
    }
    ParsedIni {
        namespace,
        sections,
        warnings,
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => quoted = !quoted,
            ';' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn local_resource_files(sections: &[IniSection]) -> BTreeMap<String, Vec<String>> {
    sections
        .iter()
        .filter(|section| section.name.to_ascii_lowercase().starts_with("resource"))
        .filter_map(|section| {
            let files = section
                .assignments
                .iter()
                .filter(|(key, _)| key == "filename")
                .map(|(_, value)| value.trim_matches('"').replace('\\', "/"))
                .collect::<Vec<_>>();
            (!files.is_empty()).then(|| (section.name.to_ascii_lowercase(), files))
        })
        .collect()
}

fn override_details(
    section: &IniSection,
    resource_files: &BTreeMap<String, Vec<String>>,
) -> String {
    let mut details = Vec::new();
    for key in ["match_index_count", "match_priority", "handling"] {
        if let Some((_, value)) = section
            .assignments
            .iter()
            .rev()
            .find(|(item, _)| item == key)
        {
            details.push(format!("{key}={value}"));
        }
    }
    let mut referenced_files = BTreeSet::new();
    for (_, value) in &section.assignments {
        for token in value.split(|character: char| {
            character.is_whitespace() || matches!(character, ',' | '(' | ')' | '=')
        }) {
            let normalized = token.trim_matches(|character: char| {
                matches!(character, '"' | '\'' | '[' | ']' | '{' | '}')
            });
            if let Some(files) = resource_files.get(&normalized.to_ascii_lowercase()) {
                referenced_files.extend(files.iter().cloned());
            }
        }
    }
    if !referenced_files.is_empty() {
        details.push(format!(
            "resources={}",
            referenced_files.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if details.is_empty() {
        String::new()
    } else {
        format!(", {}", details.join(", "))
    }
}

fn override_kind(section_name: &str) -> Option<OverrideKind> {
    let normalized = section_name.to_ascii_lowercase();
    if normalized.starts_with("textureoverride") {
        Some(OverrideKind::Texture)
    } else if normalized.starts_with("shaderoverride") {
        Some(OverrideKind::Shader)
    } else {
        None
    }
}

fn normalize_hash(value: &str) -> Option<String> {
    let token = value
        .split_whitespace()
        .next()?
        .trim_matches(|character: char| matches!(character, '"' | '\'' | ','));
    let token = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
        .unwrap_or(token);
    (token.len() >= 8 && token.len() <= 64 && token.chars().all(|item| item.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

fn normalize_namespace(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_matches('\\')
        .to_ascii_lowercase()
}

fn has_ini_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("ini"))
}

fn storage_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use uuid::Uuid;

    use crate::{
        core::conflicts::{ConflictAnalysisSubject, ConflictAnalyzer},
        models::{ConflictKind, DeploymentEntry, DeploymentManifest},
    };

    use super::EfmiIniConflictAnalyzer;

    #[tokio::test]
    async fn detects_namespace_and_override_hash_conflicts_from_efmi_fixtures()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let profile_id = Uuid::new_v4();
        let first = subject(
            directory.path(),
            profile_id,
            "First",
            0,
            "namespace = Shared\\Endmin\n[TextureOverride_Component0]\nhash = 48E5C5F7\nmatch_index_count = 120\nhandling = skip\n[Resource_Texture0]\nfilename = Textures/body.dds\n[TextureOverride_Texture0]\nhash = AABBCCDD\nthis = Resource_Texture0\n",
        )?;
        let second = subject(
            directory.path(),
            profile_id,
            "Second",
            1,
            "namespace=shared/endmin\n[TextureOverride_Component7]\nhash=48e5c5f7\nmatch_index_count=120\nhandling=skip\n[Resource_Alt]\nfilename=Textures/other.dds\n[TextureOverride_Texture7]\nhash=AABBCCDD\nthis=Resource_Alt\n[ShaderOverrideSkin]\nhash=DEADBEEF\n",
        )?;
        let third = subject(
            directory.path(),
            profile_id,
            "Third",
            2,
            "[ShaderOverrideOther]\nhash=deadbeef\n",
        )?;

        let output = EfmiIniConflictAnalyzer
            .analyze(&[first, second, third])
            .await?;

        assert_eq!(output.analyzed_ini_files, 3);
        assert_eq!(output.conflicts.len(), 4);
        assert!(
            output
                .conflicts
                .iter()
                .any(|item| item.kind == ConflictKind::EfmiNamespace)
        );
        assert!(
            output
                .conflicts
                .iter()
                .any(|item| item.kind == ConflictKind::EfmiTextureOverride)
        );
        assert!(
            output
                .conflicts
                .iter()
                .any(|item| item.kind == ConflictKind::EfmiShaderOverride)
        );
        assert!(output.conflicts.iter().any(|item| {
            item.resource_key == "texture-hash:aabbccdd"
                && item
                    .participants
                    .iter()
                    .flat_map(|participant| &participant.evidence)
                    .any(|evidence| evidence.detail.contains("resources=Textures/other.dds"))
        }));
        assert!(
            output
                .conflicts
                .iter()
                .all(|item| item.winning_mod_id.is_none())
        );
        Ok(())
    }

    #[tokio::test]
    async fn common_efmi_template_sections_do_not_create_false_conflicts()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let profile_id = Uuid::new_v4();
        let template_a = "[ResourceModName]\ntype=Buffer\ndata=\"A\"\n[Constants]\nglobal $mod_id=-1000\n[Present]\nrun=CommandListRegisterMod\n[TextureOverride_Component0]\nhash=11111111\n";
        let template_b = "[ResourceModName]\ntype=Buffer\ndata=\"B\"\n[Constants]\nglobal $mod_id=-1000\n[Present]\nrun=CommandListRegisterMod\n[TextureOverride_Component0]\nhash=22222222\n";
        let output = EfmiIniConflictAnalyzer
            .analyze(&[
                subject(directory.path(), profile_id, "A", 0, template_a)?,
                subject(directory.path(), profile_id, "B", 1, template_b)?,
            ])
            .await?;

        assert!(output.conflicts.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn refuses_to_read_ini_when_deployment_marker_is_tampered()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let item = subject(
            directory.path(),
            Uuid::new_v4(),
            "Tampered",
            0,
            "namespace=should_not_be_read\n",
        )?;
        let deployment_root = item
            .manifest
            .destination_root
            .join(&item.manifest.destination_directory);
        fs::write(deployment_root.join(".aemm-deployment.json"), b"{}")?;

        let output = EfmiIniConflictAnalyzer.analyze(&[item]).await?;

        assert_eq!(output.analyzed_ini_files, 0);
        assert!(output.conflicts.is_empty());
        assert_eq!(output.warnings.len(), 1);
        Ok(())
    }

    fn subject(
        mods_root: &std::path::Path,
        profile_id: Uuid,
        name: &str,
        load_order: u32,
        ini: &str,
    ) -> Result<ConflictAnalysisSubject, Box<dyn std::error::Error>> {
        let mod_id = Uuid::new_v4();
        let destination_directory = format!("AEMM_{}", mod_id.simple());
        let root = mods_root.join(&destination_directory);
        fs::create_dir(&root)?;
        fs::write(root.join("mod.ini"), ini)?;
        let manifest = DeploymentManifest {
            schema_version: 1,
            id: Uuid::new_v4(),
            profile_id,
            mod_id,
            strategy_id: "efmi.copy.v1".to_owned(),
            destination_root: mods_root.to_path_buf(),
            destination_directory: PathBuf::from(destination_directory),
            source_content_fingerprint: "fixture".to_owned(),
            entries: vec![DeploymentEntry {
                source_relative: PathBuf::from("mod.ini"),
                destination_relative: PathBuf::from("mod.ini"),
                size_bytes: u64::try_from(ini.len())?,
                content_hash: "a".repeat(64),
            }],
            created_at: 1,
        };
        fs::write(
            root.join(".aemm-deployment.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "kind": "aemm-efmi-deployment",
                "schemaVersion": 1,
                "manifest": &manifest,
            }))?,
        )?;
        Ok(ConflictAnalysisSubject {
            mod_name: name.to_owned(),
            load_order,
            manifest,
        })
    }
}
