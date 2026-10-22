//! Versioned user-workflow rules. The executor and transition transaction do not
//! embed stage names or a fixed number of communities.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::*;

pub(super) fn invalid(message: &str) -> DbError {
    DbError::DeletionSafety(message.into())
}

pub(super) struct Inventory {
    pub communities: Vec<(Uuid, Value)>,
}

impl Inventory {
    pub fn read(request: &EntityRequest) -> Result<Self> {
        validate_submission(
            &request.target,
            &request.requested_by,
            request.reason.as_deref(),
        )?;
        let value = request
            .inventory_manifest
            .as_ref()
            .ok_or_else(|| invalid("missing inventory"))?;
        if value["version"] != 1 || value["target"] != request.target {
            return Err(invalid("unsupported inventory version or target"));
        }
        let rows = value["communities"]
            .as_array()
            .ok_or_else(|| invalid("missing communities"))?;
        if rows.len() > 1000 {
            return Err(invalid("inventory exceeds 1000 communities"));
        }
        let mut seen = HashSet::new();
        let mut communities = Vec::new();
        for row in rows {
            let id = row["community_id"]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| invalid("invalid inventory community id"))?;
            if !seen.insert(id)
                || row["action"] != "delete_subject_data"
                || !matches!(
                    row["relationship"].as_str(),
                    Some("participant" | "former_participant")
                )
            {
                return Err(invalid("duplicate community or unsupported deletion scope"));
            }
            communities.push((id, row.clone()));
        }
        Ok(Self { communities })
    }

    pub fn inputs(&self, target: &str, community: Option<Uuid>) -> Result<Value> {
        let community = match community {
            None => Value::Null,
            Some(id) => self
                .communities
                .iter()
                .find(|(key, _)| *key == id)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| invalid("stage community is outside inventory"))?,
        };
        Ok(json!({"target": target, "community": community}))
    }
}

pub(super) fn validated_inventory_digest(request: &EntityRequest) -> Result<String> {
    Inventory::read(request)?;
    let inventory = request
        .inventory_manifest
        .as_ref()
        .ok_or_else(|| invalid("missing inventory"))?;
    let digest = inventory_digest(inventory)?;
    if request.inventory_digest.as_deref() != Some(&digest) {
        return Err(invalid("saved inventory digest changed"));
    }
    Ok(digest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "stage", rename_all = "snake_case")]
pub(super) enum Decision {
    Stage(&'static str),
    EnterCommunities,
    CommunityComplete,
    RequestComplete,
}

pub(super) struct Rule {
    pub name: &'static str,
    pub community_scope: bool,
    pub allowed: &'static [Decision],
    pub decide: fn(&Value) -> Result<Decision>,
}

pub(super) const INITIAL_STAGE: &str = "gate_user";
const COMMUNITY_INITIAL_STAGE: &str = "purge_db";
const RULES: &[Rule] = &[
    Rule {
        name: "gate_user",
        community_scope: false,
        allowed: &[Decision::EnterCommunities],
        decide: |_| Ok(Decision::EnterCommunities),
    },
    Rule {
        name: "purge_db",
        community_scope: true,
        allowed: &[Decision::Stage("purge_agents"), Decision::CommunityComplete],
        decide: after_purge_db,
    },
    Rule {
        name: "purge_agents",
        community_scope: true,
        allowed: &[Decision::CommunityComplete],
        decide: |_| Ok(Decision::CommunityComplete),
    },
];

fn after_purge_db(result: &Value) -> Result<Decision> {
    match result["has_agents"].as_bool() {
        Some(true) => Ok(Decision::Stage("purge_agents")),
        Some(false) => Ok(Decision::CommunityComplete),
        None => Err(invalid("purge_db result must contain has_agents")),
    }
}

pub(super) fn rule(name: &str) -> Result<&'static Rule> {
    RULES
        .iter()
        .find(|rule| rule.name == name)
        .ok_or_else(|| invalid("unknown workflow stage"))
}

pub(super) fn decision(stage: &EntityStage) -> Result<Decision> {
    let rule = rule(&stage.stage)?;
    let result = stage
        .metadata_json
        .get("result")
        .ok_or_else(|| invalid("completed stage has no saved result"))?;
    let next = (rule.decide)(result)?;
    if !rule.allowed.contains(&next) {
        return Err(invalid("stage selected a disallowed transition"));
    }
    Ok(next)
}

#[derive(Debug, PartialEq)]
pub(super) struct Step {
    pub name: &'static str,
    pub community: Option<Uuid>,
}

pub(super) struct Progress {
    pub current: Option<EntityStage>,
    pub completed_communities: HashSet<Uuid>,
    pub terminal: bool,
}

impl Progress {
    /// Replay saved decisions through the persisted predecessor chain. This
    /// validates chronology without relying on timestamps or UUID ordering.
    pub fn rebuild(
        request: &EntityRequest,
        inventory: &Inventory,
        history: &[EntityStage],
    ) -> Result<Self> {
        let mut progress = Self {
            current: None,
            completed_communities: HashSet::new(),
            terminal: false,
        };
        let mut successors = HashMap::new();
        for row in history {
            let predecessor: Option<Uuid> = serde_json::from_value(
                row.metadata_json
                    .get("previous_stage_id")
                    .cloned()
                    .ok_or_else(|| invalid("stage has no predecessor metadata"))?,
            )
            .map_err(|_| invalid("invalid predecessor id"))?;
            if successors.insert(predecessor, row).is_some() {
                return Err(invalid(
                    "stage history branches into multiple current positions",
                ));
            }
        }
        let mut previous = None;
        let mut expected = Some(Step {
            name: INITIAL_STAGE,
            community: None,
        });
        let mut visited = HashSet::new();
        while let Some(row) = successors.get(&previous) {
            if !visited.insert(row.id) {
                return Err(invalid("cycle in stage history"));
            }
            let step = expected
                .take()
                .ok_or_else(|| invalid("stage exists after terminal outcome"))?;
            if row.stage != step.name
                || row.target_community_id != step.community
                || rule(&row.stage)?.community_scope != row.target_community_id.is_some()
                || row.metadata_json["inputs"]
                    != inventory.inputs(&request.target, step.community)?
            {
                return Err(invalid(
                    "stage does not match saved workflow position or inputs",
                ));
            }
            previous = Some(row.id);
            if row.is_current {
                if row.metadata_json.get("transition").is_some()
                    || successors.contains_key(&previous)
                {
                    return Err(invalid("current stage already has a committed transition"));
                }
                progress.current = Some((*row).clone());
                break;
            }
            if row.status != "completed" {
                return Err(invalid("non-current stage is not completed"));
            }
            let outcome = decision(row)?;
            if row.metadata_json["transition"] != serde_json::to_value(outcome)? {
                return Err(invalid("saved transition disagrees with handler result"));
            }
            expected = progress.apply(inventory, row, outcome)?;
            progress.terminal = expected.is_none();
        }
        if visited.len() != history.len() {
            return Err(invalid("disconnected stage history"));
        }
        Ok(progress)
    }

    pub fn apply(
        &mut self,
        inventory: &Inventory,
        row: &EntityStage,
        outcome: Decision,
    ) -> Result<Option<Step>> {
        match outcome {
            Decision::Stage(name) => {
                if rule(name)?.community_scope != row.target_community_id.is_some() {
                    return Err(invalid("transition crosses incompatible stage scopes"));
                }
                Ok(Some(Step {
                    name,
                    community: row.target_community_id,
                }))
            }
            Decision::EnterCommunities | Decision::CommunityComplete => {
                if outcome == Decision::CommunityComplete {
                    let id = row
                        .target_community_id
                        .ok_or_else(|| invalid("community outcome on request stage"))?;
                    if !self.completed_communities.insert(id) {
                        return Err(invalid("community completed twice"));
                    }
                }
                match inventory
                    .communities
                    .iter()
                    .find(|(id, _)| !self.completed_communities.contains(id))
                {
                    Some((id, _)) => Ok(Some(Step {
                        name: COMMUNITY_INITIAL_STAGE,
                        community: Some(*id),
                    })),
                    None => self.apply(inventory, row, Decision::RequestComplete),
                }
            }
            Decision::RequestComplete => {
                if self.completed_communities.len() != inventory.communities.len() {
                    return Err(invalid("request cannot finish with uncovered communities"));
                }
                Ok(None)
            }
        }
    }
}
