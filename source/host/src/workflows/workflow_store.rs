use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::agents::agent_workflow_enablement::AgentWorkflowEnablement;
use crate::automations::automation::{AutomationRecord, AutomationSpec};
use crate::automations::automation_store::{
    FileAutomationStore, UserTimeZoneResolver, get_agent_automations_dir,
};
use crate::extensions::managed_setup::managed_skills_cache::{
    ManagedSkill, ManagedSkillsCache, get_managed_skill_file_path,
    get_managed_skills_cache_path, get_managed_skills_dir, read_managed_skills_cache,
};
use crate::extensions::mcp::plugin_skills_cache::{
    PluginSkillRecord, PluginSkillsCache, get_plugin_skills_cache_path,
    get_plugin_skills_dir, read_plugin_skills_cache,
};
use crate::watched_directory::{ChangeListener, WatchedDirectory};

use super::stat_keyed_parse_cache::StatKeyedParseCache;
use super::workflow_library::{
    GlobalWorkflowLibrary, GlobalWorkflowRecord, LEGACY_WORKFLOW_FILENAME,
    WorkflowSpec, WorkflowTrigger, clamp_workflow_body, clamp_workflow_name,
    parse_workflow_file, serialize_workflow_file,
};

pub const LEGACY_WORKFLOWS_DIRNAME: &str = "workflows";
pub const MANAGED_SKILLS_CHANGE_DEBOUNCE_MS: u64 = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub trigger: Option<WorkflowTrigger>,
    pub source: String,
    pub source_ref: Option<String>,
    pub plugin_id: Option<String>,
    pub published_by_current_user: bool,
    pub disable_model_invocation: bool,
    pub is_enabled_for_agent: bool,
    pub created_at: f64,
    pub last_run_at: Option<f64>,
    pub next_run_at: Option<f64>,
    pub helper_scripts: Vec<String>,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSkillFile {
    pub path: PathBuf,
    pub label: String,
    pub fallback_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowImportResult {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowImportSkipped {
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowImportBatch {
    pub imported: Vec<WorkflowImportResult>,
    pub skipped: Vec<WorkflowImportSkipped>,
}

#[derive(Debug, Clone, PartialEq)]
struct PluginSkillFacts {
    name: String,
    description: String,
    body: String,
    disable_model_invocation: bool,
    helper_scripts: Vec<String>,
}

pub struct FileWorkflowStore {
    agent_dir: PathBuf,
    pub library: GlobalWorkflowLibrary,
    pub enablement: AgentWorkflowEnablement,
    pub automations: FileAutomationStore,
    managed_dir: PathBuf,
    managed_dir_watcher: WatchedDirectory,
    plugin_skills_dir: PathBuf,
    plugin_skills_dir_watcher: WatchedDirectory,
    managed_index_cache: StatKeyedParseCache<Option<ManagedSkillsCache>>,
    plugin_index_cache: StatKeyedParseCache<Option<PluginSkillsCache>>,
    plugin_parse_cache: StatKeyedParseCache<Option<PluginSkillFacts>>,
}

impl FileWorkflowStore {
    pub fn new(agent_dir: impl Into<PathBuf>, global_dir: impl Into<PathBuf>) -> Self {
        Self::with_user_time_zone_resolver(agent_dir, global_dir, Arc::new(|| None))
    }

    pub fn with_user_time_zone_resolver(
        agent_dir: impl Into<PathBuf>,
        global_dir: impl Into<PathBuf>,
        resolve_user_time_zone: UserTimeZoneResolver,
    ) -> Self {
        let agent_dir = agent_dir.into();
        let global_dir = global_dir.into();
        let sand_root = global_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| global_dir.clone());
        let managed_dir = get_managed_skills_dir(&sand_root);
        let plugin_skills_dir = get_plugin_skills_dir(&sand_root);
        let value = Self {
            library: GlobalWorkflowLibrary::new(global_dir),
            enablement: AgentWorkflowEnablement::new(agent_dir.clone()),
            automations: FileAutomationStore::with_user_time_zone_resolver(
                get_agent_automations_dir(&agent_dir),
                resolve_user_time_zone,
            ),
            managed_dir_watcher: WatchedDirectory::new(
                managed_dir.clone(),
                MANAGED_SKILLS_CHANGE_DEBOUNCE_MS,
            ),
            plugin_skills_dir_watcher: WatchedDirectory::new(
                plugin_skills_dir.clone(),
                MANAGED_SKILLS_CHANGE_DEBOUNCE_MS,
            ),
            managed_dir,
            plugin_skills_dir,
            managed_index_cache: StatKeyedParseCache::default(),
            plugin_index_cache: StatKeyedParseCache::default(),
            plugin_parse_cache: StatKeyedParseCache::default(),
            agent_dir,
        };
        value.migrate_legacy_per_agent_workflows();
        value
    }

    pub fn get_location(&self) -> &Path {
        self.library.get_location()
    }

    pub fn set_on_change(&self, on_change: Option<ChangeListener>) {
        self.library.set_on_change(on_change.clone());
        let _ = self.managed_dir_watcher.set_on_change(on_change.clone());
        let _ = self.plugin_skills_dir_watcher.set_on_change(on_change.clone());
        self.automations.set_on_change(on_change);
    }

    fn base_workflow(
        &self,
        id: String,
        name: String,
        description: String,
        body: String,
        source: &str,
        source_ref: Option<String>,
        created_at: f64,
        file_path: PathBuf,
        helper_scripts: Vec<String>,
    ) -> WorkflowRecord {
        WorkflowRecord {
            id,
            name,
            description,
            body,
            trigger: None,
            source: source.into(),
            source_ref,
            plugin_id: None,
            published_by_current_user: false,
            disable_model_invocation: false,
            is_enabled_for_agent: true,
            created_at,
            last_run_at: None,
            next_run_at: None,
            helper_scripts,
            file_path,
        }
    }

    fn skill_to_workflow(&self, record: GlobalWorkflowRecord) -> WorkflowRecord {
        let id = record.id.clone();
        let mut workflow = self.base_workflow(
            record.id,
            record.name,
            record.description,
            record.body,
            "workflow",
            record.source_ref,
            record.created_at,
            record.file_path,
            record.helper_scripts,
        );
        workflow.is_enabled_for_agent = self.enablement.is_enabled(&id);
        workflow
    }

    fn automation_to_workflow(&self, automation: AutomationRecord) -> WorkflowRecord {
        WorkflowRecord {
            id: automation.id,
            name: automation.name,
            description: String::new(),
            body: automation.prompt,
            trigger: Some(WorkflowTrigger {
                schedule: automation.schedule,
                is_enabled: automation.is_enabled,
            }),
            source: "automation".into(),
            source_ref: None,
            plugin_id: None,
            published_by_current_user: false,
            disable_model_invocation: false,
            is_enabled_for_agent: true,
            created_at: automation.created_at,
            last_run_at: automation.last_run_at,
            next_run_at: automation.next_run_at,
            helper_scripts: Vec::new(),
            file_path: automation.file_path,
        }
    }

    fn managed_skill_to_workflow(
        &self,
        skill: ManagedSkill,
        fetched_at: f64,
    ) -> WorkflowRecord {
        let skill_file = get_managed_skill_file_path(&self.managed_dir, &skill.id);
        let file_path = if skill_file.is_file() {
            skill_file
        } else {
            get_managed_skills_cache_path(&self.managed_dir)
        };
        self.base_workflow(
            skill.id,
            skill.name,
            skill.description,
            skill.body,
            "managed",
            None,
            fetched_at,
            file_path,
            Vec::new(),
        )
    }

    fn read_managed_skills_index(&self) -> Option<ManagedSkillsCache> {
        let path = get_managed_skills_cache_path(&self.managed_dir);
        self.managed_index_cache
            .read(std::slice::from_ref(&path), || {
                read_managed_skills_cache(&self.managed_dir)
            })
            .flatten()
    }

    fn managed_workflows(&self, excluded_ids: &BTreeSet<String>) -> Vec<WorkflowRecord> {
        self.read_managed_skills_index()
            .map(|cache| {
                cache
                    .skills
                    .into_iter()
                    .filter(|skill| !excluded_ids.contains(&skill.id))
                    .map(|skill| self.managed_skill_to_workflow(skill, cache.fetched_at))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_plugin_skills_index(&self) -> Option<PluginSkillsCache> {
        let path = get_plugin_skills_cache_path(&self.plugin_skills_dir);
        self.plugin_index_cache
            .read(std::slice::from_ref(&path), || {
                read_plugin_skills_cache(&self.plugin_skills_dir)
            })
            .flatten()
    }

    fn read_plugin_skill_facts(&self, path: &Path) -> Option<PluginSkillFacts> {
        let parent = path.parent()?.to_path_buf();
        let stat_paths = vec![path.to_path_buf(), parent.clone()];
        self.plugin_parse_cache
            .read(&stat_paths, || {
                let raw = fs::read_to_string(path).ok()?;
                let parsed = parse_workflow_file(&raw)?;
                if parsed.spec.body.is_empty() {
                    return None;
                }
                let mut helper_scripts = fs::read_dir(&parent)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_type().map(|kind| kind.is_file()).unwrap_or(false)
                            && entry.file_name() != "SKILL.md"
                    })
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>();
                helper_scripts.sort();
                Some(PluginSkillFacts {
                    name: parsed.spec.name,
                    description: parsed.spec.description,
                    body: parsed.spec.body,
                    disable_model_invocation: parsed
                        .data
                        .get("disable-model-invocation")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true),
                    helper_scripts,
                })
            })
            .flatten()
    }

    fn plugin_skill_to_workflow(
        &self,
        record: PluginSkillRecord,
        index: &PluginSkillsCache,
    ) -> Option<WorkflowRecord> {
        let path = PathBuf::from(&record.file_path);
        let facts = self.read_plugin_skill_facts(&path)?;
        let mut workflow = self.base_workflow(
            record.id,
            if facts.name.is_empty() {
                record.name
            } else {
                facts.name
            },
            if facts.description.is_empty() {
                record.description
            } else {
                facts.description
            },
            facts.body,
            "plugin",
            None,
            index.fetched_at,
            path,
            facts.helper_scripts,
        );
        workflow.plugin_id = Some(record.plugin_id);
        workflow.published_by_current_user =
            record.publisher_user_id.is_some() && record.publisher_user_id == index.current_user_id;
        workflow.disable_model_invocation = facts.disable_model_invocation;
        Some(workflow)
    }

    fn plugin_workflows(&self, excluded_ids: &BTreeSet<String>) -> Vec<WorkflowRecord> {
        let Some(index) = self.read_plugin_skills_index() else {
            return Vec::new();
        };
        index
            .skills
            .iter()
            .filter(|record| !excluded_ids.contains(&record.id))
            .filter_map(|record| self.plugin_skill_to_workflow(record.clone(), &index))
            .collect()
    }

    fn plugin_skill_record(&self, id: &str) -> Option<(PluginSkillRecord, PluginSkillsCache)> {
        let index = self.read_plugin_skills_index()?;
        let record = index.skills.iter().find(|record| record.id == id)?.clone();
        Some((record, index))
    }

    pub fn list_all(&self) -> Vec<WorkflowRecord> {
        self.list_all_from(false)
    }

    pub fn list_all_from(&self, definitions_only: bool) -> Vec<WorkflowRecord> {
        let user_records = self.library.list();
        let user_ids = user_records
            .iter()
            .map(|record| record.id.clone())
            .collect::<BTreeSet<_>>();
        let user = user_records
            .into_iter()
            .map(|record| self.skill_to_workflow(record))
            .collect::<Vec<_>>();
        let managed = self.managed_workflows(&user_ids);
        let claimed = user_ids
            .iter()
            .cloned()
            .chain(managed.iter().map(|workflow| workflow.id.clone()))
            .collect::<BTreeSet<_>>();
        let plugins = self.plugin_workflows(&claimed);
        let automations = if definitions_only {
            self.automations.list_definitions()
        } else {
            self.automations.list()
        }
        .into_iter()
        .map(|automation| self.automation_to_workflow(automation));
        managed
            .into_iter()
            .chain(plugins)
            .chain(user)
            .chain(automations)
            .collect()
    }

    pub fn list(&self) -> Vec<WorkflowRecord> {
        self.list_all_from(true)
            .into_iter()
            .filter(|workflow| workflow.is_enabled_for_agent || workflow.source == "automation")
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<WorkflowRecord> {
        if let Some(record) = self.library.get(id) {
            return Some(self.skill_to_workflow(record));
        }
        if let Some(workflow) = self
            .managed_workflows(&BTreeSet::new())
            .into_iter()
            .find(|workflow| workflow.id == id)
        {
            return Some(workflow);
        }
        if let Some(workflow) = self
            .plugin_workflows(&BTreeSet::new())
            .into_iter()
            .find(|workflow| workflow.id == id)
        {
            return Some(workflow);
        }
        self.automations
            .get(id)
            .map(|automation| self.automation_to_workflow(automation))
    }

    pub fn create(&self, spec: &WorkflowSpec) -> Result<Option<WorkflowRecord>, String> {
        let name = clamp_workflow_name(&spec.name);
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(trigger) = &spec.trigger {
            let automation = self
                .automations
                .upsert(
                    &AutomationSpec {
                        name,
                        prompt: spec.body.clone(),
                        trigger: json!({"type":"cron","schedule":trigger.schedule}),
                        is_enabled: Some(trigger.is_enabled),
                    },
                    now_ms(),
                )
                .map_err(|error| error.to_string())?;
            return Ok(automation.map(|automation| self.automation_to_workflow(automation)));
        }
        let body = clamp_workflow_body(&spec.body);
        if body.is_empty() {
            return Ok(None);
        }
        let normalized = WorkflowSpec {
            name,
            description: spec.description.clone(),
            body,
            trigger: None,
            source_ref: spec.source_ref.clone(),
        };
        let Some(record) = self
            .library
            .create(&normalized)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        self.enablement
            .set_enabled(&record.id, true)
            .map_err(|error| error.to_string())?;
        Ok(Some(self.skill_to_workflow(record)))
    }

    pub fn update(
        &self,
        id: &str,
        spec: &WorkflowSpec,
    ) -> Result<Option<WorkflowRecord>, String> {
        let Some(current) = self.get(id) else {
            return Ok(None);
        };
        let name = clamp_workflow_name(&spec.name);
        if name.is_empty() {
            return Ok(None);
        }
        if current.source == "automation" {
            let schedule = spec
                .trigger
                .as_ref()
                .map(|trigger| trigger.schedule.clone())
                .or_else(|| current.trigger.as_ref().map(|trigger| trigger.schedule.clone()))
                .unwrap_or_default();
            let enabled = spec
                .trigger
                .as_ref()
                .map(|trigger| trigger.is_enabled)
                .or_else(|| current.trigger.as_ref().map(|trigger| trigger.is_enabled))
                .unwrap_or(true);
            let updated = self
                .automations
                .update(
                    id,
                    &AutomationSpec {
                        name,
                        prompt: spec.body.clone(),
                        trigger: json!({"type":"cron","schedule":schedule}),
                        is_enabled: Some(enabled),
                    },
                )
                .map_err(|error| error.to_string())?;
            return Ok(updated.map(|automation| self.automation_to_workflow(automation)));
        }

        let body = clamp_workflow_body(&spec.body);
        if body.is_empty() {
            return Ok(None);
        }
        let normalized = WorkflowSpec {
            name,
            description: spec.description.clone(),
            body,
            trigger: None,
            source_ref: spec.source_ref.clone().or(current.source_ref),
        };
        if current.source == "plugin" {
            if !current.published_by_current_user {
                return Ok(None);
            }
            return self.update_plugin_skill(id, &normalized);
        }
        if current.source == "managed" {
            return Ok(None);
        }
        Ok(self
            .library
            .update(id, &normalized)
            .map_err(|error| error.to_string())?
            .map(|record| self.skill_to_workflow(record)))
    }

    fn update_plugin_skill(
        &self,
        id: &str,
        spec: &WorkflowSpec,
    ) -> Result<Option<WorkflowRecord>, String> {
        let Some((record, index)) = self.plugin_skill_record(id) else {
            return Ok(None);
        };
        if record.publisher_user_id.is_none() || record.publisher_user_id != index.current_user_id {
            return Ok(None);
        }
        let raw = fs::read_to_string(&record.file_path).map_err(|error| error.to_string())?;
        let Some(parsed) = parse_workflow_file(&raw) else {
            return Ok(None);
        };
        let encoded = serialize_workflow_file(spec, &parsed.data);
        self.plugin_skills_dir_watcher
            .write_file_atomic(&record.file_path, encoded.as_bytes())
            .map_err(|error| error.to_string())?;
        Ok(self.get(id))
    }

    pub fn set_enabled_for_agent(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<WorkflowRecord>, String> {
        if let Some(workflow) = self.get(id) {
            if workflow.source == "managed" || workflow.source == "plugin" {
                return Ok(Some(workflow));
            }
        }
        if self.library.get(id).is_none() {
            return Ok(None);
        }
        self.enablement
            .set_enabled(id, enabled)
            .map_err(|error| error.to_string())?;
        Ok(self.get(id))
    }

    pub fn set_trigger_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<WorkflowRecord>, String> {
        Ok(self
            .automations
            .set_enabled(id, enabled)
            .map_err(|error| error.to_string())?
            .map(|automation| self.automation_to_workflow(automation)))
    }

    pub fn remove(&self, id: &str) -> Result<bool, String> {
        if let Some(workflow) = self.get(id) {
            if workflow.source == "managed" || workflow.source == "plugin" {
                return Ok(false);
            }
        }
        if self.library.get(id).is_some() {
            let _ = self
                .enablement
                .forget(id)
                .map_err(|error| error.to_string())?;
            return self.library.remove(id).map_err(|error| error.to_string());
        }
        self.automations.remove(id).map_err(|error| error.to_string())
    }

    pub fn import_markdown(
        &self,
        markdown: &str,
        fallback: Option<&str>,
    ) -> Result<Option<(String, String)>, String> {
        let Some(parsed) = parse_workflow_file(markdown) else {
            return Ok(None);
        };
        let name = if parsed.spec.name.is_empty() {
            derive_name(&parsed.spec.body)
                .or_else(|| fallback.map(ToOwned::to_owned))
                .unwrap_or_default()
        } else {
            parsed.spec.name
        };
        if name.is_empty() || parsed.spec.body.is_empty() {
            return Ok(None);
        }
        let spec = WorkflowSpec {
            name,
            description: parsed.spec.description,
            body: parsed.spec.body,
            trigger: None,
            source_ref: parsed.spec.source_ref,
        };
        let Some(record) = self.library.create(&spec).map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        self.enablement
            .set_enabled(&record.id, true)
            .map_err(|error| error.to_string())?;
        Ok(Some((record.id, record.name)))
    }

    pub fn import_live_source(
        &self,
        source: &str,
        fallback: Option<&str>,
    ) -> Result<Option<(String, String)>, String> {
        let source = source.trim();
        if source.is_empty() {
            return Ok(None);
        }
        let name = clamp_workflow_name(
            fallback.unwrap_or_else(|| source.rsplit('/').next().unwrap_or("Imported skill")),
        );
        if name.is_empty() {
            return Ok(None);
        }
        let body = format!(
            "This workflow is a live reference to the skill at {source}.\nRead that source now with your file or fetch tools and follow it as written. Do not assume its contents from this note; the source is the source of truth and may have changed since this workflow was created."
        );
        let spec = WorkflowSpec {
            name: name.clone(),
            description: format!(
                "Use when the \"{name}\" skill applies; it is a live reference to {source}."
            ),
            body,
            trigger: None,
            source_ref: Some(source.to_string()),
        };
        let Some(record) = self.library.create(&spec).map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        self.enablement
            .set_enabled(&record.id, true)
            .map_err(|error| error.to_string())?;
        Ok(Some((record.id, record.name)))
    }

    pub fn port_local_skills(
        &self,
        home_dir: &Path,
        cwd: &Path,
    ) -> Result<WorkflowImportBatch, String> {
        let mut batch = WorkflowImportBatch::default();
        for source in discover_local_skill_files(home_dir, cwd) {
            match self.import_live_source(
                &source.path.to_string_lossy(),
                Some(&source.fallback_name),
            )? {
                Some((id, name)) => batch.imported.push(WorkflowImportResult { id, name }),
                None => batch.skipped.push(WorkflowImportSkipped {
                    source: source.label,
                    reason: "could not link".into(),
                }),
            }
        }
        Ok(batch)
    }

    pub fn migrate_legacy_per_agent_workflows(&self) {
        let legacy = self.agent_dir.join(LEGACY_WORKFLOWS_DIRNAME);
        let Ok(entries) = fs::read_dir(&legacy) else {
            return;
        };
        let mut enabled = Vec::new();
        for entry in entries.flatten() {
            if !entry
                .file_type()
                .map(|kind| kind.is_dir())
                .unwrap_or(false)
            {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let folder = entry.path();
            if !self.library.has(&id) {
                let target = self.library.get_location().join(&id);
                if fs::create_dir_all(self.library.get_location()).is_ok()
                    && fs::rename(&folder, &target).is_err()
                {
                    let Ok(raw) = fs::read_to_string(folder.join(LEGACY_WORKFLOW_FILENAME)) else {
                        continue;
                    };
                    let Some(parsed) = parse_workflow_file(&raw) else {
                        continue;
                    };
                    if parsed.spec.body.is_empty() {
                        continue;
                    }
                    let mut spec = parsed.spec;
                    if spec.name.is_empty() {
                        spec.name = id.clone();
                    }
                    let _ = self.library.write_at(&id, &spec);
                }
            }
            enabled.push(id);
        }
        self.library.rename_legacy_recipe_files();
        if !enabled.is_empty() {
            let _ = self.enablement.enable_all(&enabled);
        }
        let _ = fs::remove_dir_all(legacy);
    }
}

fn push_local_skill_file(
    sources: &mut Vec<LocalSkillFile>,
    seen: &mut BTreeSet<PathBuf>,
    file: PathBuf,
    label: String,
    fallback_name: String,
) {
    if !seen.insert(file.clone()) {
        return;
    }
    if fs::metadata(&file)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
    {
        sources.push(LocalSkillFile {
            path: file,
            label,
            fallback_name,
        });
    }
}

pub fn discover_local_skill_files(home_dir: &Path, cwd: &Path) -> Vec<LocalSkillFile> {
    let mut sources = Vec::new();
    let mut seen = BTreeSet::<PathBuf>::new();
    for dir in [cwd, home_dir] {
        push_local_skill_file(
            &mut sources,
            &mut seen,
            dir.join("CLAUDE.md"),
            format!("{}/CLAUDE.md", dir.display()),
            "Claude memory".into(),
        );
        push_local_skill_file(
            &mut sources,
            &mut seen,
            dir.join("AGENTS.md"),
            format!("{}/AGENTS.md", dir.display()),
            "Agents memory".into(),
        );
        push_local_skill_file(
            &mut sources,
            &mut seen,
            dir.join(".claude").join("CLAUDE.md"),
            format!("{}/.claude/CLAUDE.md", dir.display()),
            "Claude memory".into(),
        );
    }
    let rules_dir = cwd.join(".cursor").join("rules");
    if let Ok(entries) = fs::read_dir(&rules_dir) {
        for entry in entries.flatten() {
            if !entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            let extension_len = if lower.ends_with(".mdc") {
                4
            } else if lower.ends_with(".md") {
                3
            } else {
                continue;
            };
            let fallback = name[..name.len() - extension_len].to_string();
            push_local_skill_file(
                &mut sources,
                &mut seen,
                entry.path(),
                format!(".cursor/rules/{name}"),
                fallback,
            );
        }
    }
    sources
}

pub fn agent_has_workflows(agent_dir: &Path) -> bool {
    if AgentWorkflowEnablement::new(agent_dir.to_path_buf()).has_explicit_entries() {
        return true;
    }
    let legacy = agent_dir.join(LEGACY_WORKFLOWS_DIRNAME);
    let Ok(entries) = fs::read_dir(legacy) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or(false)
            && entry.path().join(LEGACY_WORKFLOW_FILENAME).is_file()
    })
}

fn derive_name(body: &str) -> Option<String> {
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let text = line
            .trim_start_matches('#')
            .trim()
            .replace(['*', '_', '>'], "")
            .replace(char::from(96), "");
        if !text.is_empty() {
            return Some(text.chars().take(80).collect());
        }
    }
    None
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
