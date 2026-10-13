use super::managed_skills_cache::{is_safe_managed_skill_id, ManagedSkill};

const WORKFLOW_MAX_NAME_LENGTH: usize = 80;
const WORKFLOW_MAX_DESCRIPTION_LENGTH: usize = 1_536;
const WORKFLOW_MAX_BODY_LENGTH: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedManagedSkill {
    pub id: String,
    pub description: String,
    pub content: String,
    pub enabled: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ParsedWorkflow { name: String, description: String, body: String }

pub fn fetched_managed_skill_to_sand_skill(skill: &FetchedManagedSkill) -> Option<ManagedSkill> {
    if !skill.enabled { return None; }
    let id = skill.id.trim();
    if !is_safe_managed_skill_id(id) { return None; }
    let parsed = parse_workflow_file(&skill.content);
    let body = clamp_block(parsed.as_ref().map(|p| p.body.as_str()).unwrap_or(skill.content.as_str()), WORKFLOW_MAX_BODY_LENGTH);
    if body.trim().is_empty() { return None; }
    let parsed_name = parsed.as_ref().map(|p| p.name.as_str()).unwrap_or_default();
    let name = clamp_line(if parsed_name.is_empty() { id } else { parsed_name }, WORKFLOW_MAX_NAME_LENGTH);
    if name.is_empty() { return None; }
    let parsed_description = parsed.as_ref().map(|p| p.description.as_str()).unwrap_or_default();
    let description = clamp_line(if parsed_description.is_empty() { skill.description.as_str() } else { parsed_description }, WORKFLOW_MAX_DESCRIPTION_LENGTH);
    Some(ManagedSkill { id:id.to_string(), name, description, body })
}

fn clamp_line(value:&str,max:usize)->String { value.split(['\r','\n']).filter(|p|!p.is_empty()).collect::<Vec<_>>().join(" ").trim().chars().take(max).collect() }
fn clamp_block(value:&str,max:usize)->String { value.trim().chars().take(max).collect() }
fn parse_workflow_file(raw:&str)->Option<ParsedWorkflow> {
    if !raw.starts_with("---") { let body=clamp_block(raw,WORKFLOW_MAX_BODY_LENGTH); return (!body.is_empty()).then_some(ParsedWorkflow{body,..Default::default()}); }
    let line_end=raw.find('\n')?; let after_open=&raw[line_end+1..]; let relative_close=after_open.find("\n---")?; let frontmatter=&after_open[..relative_close];
    let mut body=&after_open[relative_close+4..]; body=body.strip_prefix("\r\n").or_else(||body.strip_prefix('\n')).unwrap_or(body);
    let mut parsed=ParsedWorkflow{body:clamp_block(body,WORKFLOW_MAX_BODY_LENGTH),..Default::default()};
    for line in frontmatter.lines() { let Some((key,value))=line.split_once(':') else { continue; }; let value=parse_scalar_string(value.trim()); match key.trim(){"name"=>parsed.name=clamp_line(&value,WORKFLOW_MAX_NAME_LENGTH),"description"=>parsed.description=clamp_line(&value,WORKFLOW_MAX_DESCRIPTION_LENGTH),_=>{}} }
    Some(parsed)
}
fn parse_scalar_string(value:&str)->String {
    if value.len()>=2 && value.starts_with('"') && value.ends_with('"') { return serde_json::from_str::<String>(value).unwrap_or_else(|_|value[1..value.len()-1].to_string()); }
    if value.len()>=2 && value.starts_with('\'') && value.ends_with('\'') { return value[1..value.len()-1].replace("''", "'"); }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_unsafe_and_empty_skills_fail_closed() {
        let base=FetchedManagedSkill{id:"skill".into(),description:"d".into(),content:"body".into(),enabled:true};
        assert!(fetched_managed_skill_to_sand_skill(&base).is_some());
        assert!(fetched_managed_skill_to_sand_skill(&FetchedManagedSkill{enabled:false,..base.clone()}).is_none());
        assert!(fetched_managed_skill_to_sand_skill(&FetchedManagedSkill{id:"../escape".into(),..base.clone()}).is_none());
        assert!(fetched_managed_skill_to_sand_skill(&FetchedManagedSkill{content:"   ".into(),..base}).is_none());
    }
    #[test]
    fn workflow_frontmatter_is_bounded_and_body_remains_agent_editable_markdown() {
        let skill=FetchedManagedSkill{id:"review".into(),description:"fallback".into(),content:"---\nname: \"Review\"\ndescription: 'Team rule'\n---\nCheck the change.".into(),enabled:true};
        let parsed=fetched_managed_skill_to_sand_skill(&skill).unwrap();
        assert_eq!(parsed.name,"Review"); assert_eq!(parsed.description,"Team rule"); assert_eq!(parsed.body,"Check the change.");
    }
}
