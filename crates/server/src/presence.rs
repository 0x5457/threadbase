//! Ephemeral presence for project focus sharing. Never persisted or replayed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use threadbase_shared::{Focus, ProjectId, ServerWsMessage};
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Default)]
struct ProjectPresence {
    focuses: HashMap<String, Focus>,
    online: HashSet<String>,
    dirty: bool,
    flushing: bool,
}

#[derive(Clone)]
pub struct PresenceHub {
    projects: Arc<RwLock<HashMap<String, ProjectPresence>>>,
    tx: broadcast::Sender<ServerWsMessage>,
}

impl Default for PresenceHub {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenceHub {
    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self {
            projects: Arc::new(RwLock::new(HashMap::new())),
            tx,
        }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerWsMessage> {
        self.tx.subscribe()
    }

    pub async fn update(&self, focus: Focus) {
        let project = focus.project_id.0.clone();
        {
            let mut projects = self.projects.write().await;
            let entry = projects.entry(project.clone()).or_default();
            entry.online.insert(focus.user_id.0.clone());
            entry.focuses.insert(focus.user_id.0.clone(), focus);
            entry.dirty = true;
        }
        self.schedule(project);
    }

    /// Coalesce updates into at most one broadcast per project every 50 ms.
    fn schedule(&self, project_id: String) {
        let projects = self.projects.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            {
                let mut projects = projects.write().await;
                let Some(entry) = projects.get_mut(&project_id) else {
                    return;
                };
                if entry.flushing {
                    return;
                }
                entry.flushing = true;
            }
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let message = {
                    let mut projects = projects.write().await;
                    let Some(entry) = projects.get_mut(&project_id) else {
                        return;
                    };
                    entry.dirty = false;
                    snapshot(&project_id, entry)
                };
                let _ = tx.send(message);
                let more = {
                    let mut projects = projects.write().await;
                    let Some(entry) = projects.get_mut(&project_id) else {
                        return;
                    };
                    if entry.dirty {
                        true
                    } else {
                        entry.flushing = false;
                        false
                    }
                };
                if !more {
                    break;
                }
            }
        });
    }

    /// Online users for a project, used to annotate API reads.
    pub async fn online_user_ids(&self, project_id: &str) -> HashSet<String> {
        let projects = self.projects.read().await;
        projects
            .get(project_id)
            .map(|entry| entry.online.clone())
            .unwrap_or_default()
    }

    pub async fn remove_user(&self, user_id: &str) {
        let projects_to_flush = {
            let mut projects = self.projects.write().await;
            let mut changed = Vec::new();
            for (project_id, entry) in projects.iter_mut() {
                if entry.online.remove(user_id) || entry.focuses.remove(user_id).is_some() {
                    entry.dirty = true;
                    changed.push(project_id.clone());
                }
            }
            projects.retain(|_, entry| !entry.online.is_empty() || !entry.focuses.is_empty());
            changed
        };
        for project in projects_to_flush {
            self.schedule(project);
        }
    }

    pub async fn snapshot(&self, project_id: &ProjectId) -> ServerWsMessage {
        let projects = self.projects.read().await;
        match projects.get(project_id.as_str()) {
            Some(entry) => snapshot(project_id.as_str(), entry),
            None => ServerWsMessage::Presence {
                project_id: project_id.clone(),
                focuses: Vec::new(),
                online_user_ids: Vec::new(),
            },
        }
    }
}

fn snapshot(project_id: &str, entry: &ProjectPresence) -> ServerWsMessage {
    let mut focuses: Vec<Focus> = entry
        .focuses
        .values()
        .filter(|focus| !focus.hidden)
        .cloned()
        .collect();
    focuses.sort_by_key(|focus| focus.user_id.0.clone());
    let mut online: Vec<String> = entry.online.iter().cloned().collect();
    online.sort();
    ServerWsMessage::Presence {
        project_id: ProjectId::from(project_id),
        focuses,
        online_user_ids: online.into_iter().map(Into::into).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::{now_ms, Anchor, Panel, UserId};

    fn focus(user: &str, project: &str, hidden: bool) -> Focus {
        Focus {
            user_id: UserId::from(user),
            project_id: ProjectId::from(project),
            thread_id: None,
            panel: Panel::Transcript,
            target: Some(Anchor::Project {
                project_id: "p1".into(),
            }),
            typing: false,
            hidden,
            updated_at: now_ms(),
        }
    }

    #[tokio::test]
    async fn hidden_focus_is_not_broadcast_but_user_stays_online() {
        let hub = PresenceHub::new();
        let mut rx = hub.subscribe();
        hub.update(focus("u1", "p1", false)).await;
        let message = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let ServerWsMessage::Presence { focuses, .. } = message else {
            panic!("unexpected message");
        };
        assert_eq!(focuses.len(), 1);
        hub.update(focus("u2", "p1", true)).await;
        let message = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let ServerWsMessage::Presence {
            focuses,
            online_user_ids,
            ..
        } = message
        else {
            panic!("unexpected message");
        };
        assert_eq!(focuses.len(), 1);
        assert_eq!(online_user_ids.len(), 2);
    }
}
