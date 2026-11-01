//! What a candidate is related to (ADR-0053): the relationships in its
//! decision's evidence bundle, bounded, with nodes the UI can match to other
//! candidates. Only edges the graph has; none is inferred from paths, names
//! or groups.

use lumen_application::assess::Assessed;
use lumen_domain::{FileIdentity, RelationKind, Relationship, Subject};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// At most this many relationships are shown per candidate.
pub const MAX_RELATIONS: usize = 16;

/// The opaque ID of the filesystem object `identity`: the same for every
/// path to it (hard links), stable while the platform reports the same
/// volume and file IDs (a macOS device number can change when a volume is
/// mounted again), never made from a path.
pub(super) fn file_id(identity: &FileIdentity) -> String {
    // A volume ID is visible ASCII and a file ID a number, so the last `:`
    // separates them and the ID is unambiguous.
    format!("{}:{}", identity.volume, identity.file)
}

/// The kind of a relationship, read `from <kind> to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationKindView {
    /// A folder or volume contains an entry.
    Contains,
    /// A file belongs to an application.
    OwnedBy,
    /// A file was created by an application.
    CreatedBy,
    /// A file is open in a running process.
    OpenedBy,
    /// A process was started by a service.
    LaunchedBy,
    /// A service runs this executable.
    Launches,
    /// A vendor declared a file a disposable cache of an application.
    DeclaredCacheOf,
    /// Two files share blocks.
    CloneOf,
    /// Two files have identical content.
    DuplicateOf,
    /// A file's content lives inside another file.
    StoredIn,
}

/// One end of a relationship. Untrusted text is escaped for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeView {
    /// A filesystem object: this candidate, another candidate with the same
    /// `id`, or one Lumen did not assess (then no path is known here).
    File {
        /// Its opaque ID, as in `CandidateView.id`.
        id: String,
    },
    /// An installed or once-installed application.
    Application {
        /// Its identifier, such as `com.example.app`.
        app: String,
    },
    /// A running process.
    Process {
        /// Its process ID, as a decimal string.
        pid: String,
    },
    /// A background, startup or scheduled component.
    Service {
        /// Its label or name.
        label: String,
    },
    /// A volume.
    Volume {
        /// Its opaque ID.
        id: String,
    },
}

/// One relationship in the candidate's evidence bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RelationView {
    /// Where it starts.
    pub from: NodeView,
    /// What it says.
    pub kind: RelationKindView,
    /// Where it ends.
    pub to: NodeView,
    /// 1 when it touches the candidate, 2 when it is one step further.
    pub hops: u8,
}

/// The candidate's relationships, nearest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RelationsView {
    /// At most 16, the nearest first, in a fixed order. Empty means none was
    /// recorded, not that it is unrelated to anything.
    pub edges: Vec<RelationView>,
    /// How many more there are, as a decimal string: left out by the bound,
    /// or of a kind this view does not know.
    pub omitted: String,
}

fn node(subject: &Subject) -> Option<NodeView> {
    Some(match subject {
        Subject::File { identity } => NodeView::File {
            id: file_id(identity),
        },
        Subject::Application { id } => NodeView::Application {
            app: id.value.display(),
        },
        Subject::Process { pid, .. } => NodeView::Process {
            pid: pid.to_string(),
        },
        Subject::Service { label, .. } => NodeView::Service {
            label: label.display(),
        },
        Subject::Volume { id } => NodeView::Volume { id: id.to_string() },
        _ => return None,
    })
}

const fn kind(kind: RelationKind) -> Option<RelationKindView> {
    Some(match kind {
        RelationKind::Contains => RelationKindView::Contains,
        RelationKind::OwnedBy => RelationKindView::OwnedBy,
        RelationKind::CreatedBy => RelationKindView::CreatedBy,
        RelationKind::OpenedBy => RelationKindView::OpenedBy,
        RelationKind::LaunchedBy => RelationKindView::LaunchedBy,
        RelationKind::Launches => RelationKindView::Launches,
        RelationKind::DeclaredCacheOf => RelationKindView::DeclaredCacheOf,
        RelationKind::CloneOf => RelationKindView::CloneOf,
        RelationKind::DuplicateOf => RelationKindView::DuplicateOf,
        RelationKind::StoredIn => RelationKindView::StoredIn,
        _ => return None,
    })
}

fn view(relationship: &Relationship, hops: u8) -> Option<RelationView> {
    Some(RelationView {
        from: node(relationship.from())?,
        kind: kind(relationship.kind())?,
        to: node(relationship.to())?,
        hops,
    })
}

/// The relationships of `assessed`'s bundle: those touching the candidate
/// first, each group in the domain's order, at most [`MAX_RELATIONS`].
pub(super) fn relations(assessed: &Assessed) -> RelationsView {
    let candidate = Subject::File {
        identity: assessed.candidate.identity().clone(),
    };
    let hops = |relationship: &Relationship| {
        if relationship.from() == &candidate || relationship.to() == &candidate {
            1
        } else {
            2
        }
    };
    let mut ordered: Vec<&Relationship> = assessed.relationships.iter().collect();
    ordered.sort_by_key(|relationship| (hops(relationship), *relationship));
    let known: Vec<RelationView> = ordered
        .iter()
        .filter_map(|relationship| view(relationship, hops(relationship)))
        .collect();
    let edges: Vec<RelationView> = known.into_iter().take(MAX_RELATIONS).collect();
    RelationsView {
        omitted: (ordered.len() - edges.len()).to_string(),
        edges,
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{AccessState, EntryKind, Fact};

    use super::*;
    use crate::views::fixture::{app_id, assessed, file, record};

    fn owned_by(n: u128, app: &str) -> Relationship {
        let owner = record(file(n), Fact::OwningApplication(app_id(app)), 1);
        Relationship::new(
            file(n),
            RelationKind::OwnedBy,
            Subject::Application { id: app_id(app) },
            [owner.id()],
        )
        .unwrap_or_else(|e| unreachable!("{e:?}"))
    }

    #[test]
    fn relations_are_the_bundles_edges_nearest_first_with_ids() {
        let mut item = assessed(EntryKind::Directory, AccessState::Readable, Vec::new());
        item.relationships = vec![
            owned_by(2, "com.example.app"),
            owned_by(1, "com.example.app"),
        ];
        let shown = relations(&item);
        let app = || NodeView::Application {
            app: "com.example.app".to_owned(),
        };
        assert_eq!(
            shown,
            RelationsView {
                edges: vec![
                    RelationView {
                        from: NodeView::File {
                            id: "v:1".to_owned()
                        },
                        kind: RelationKindView::OwnedBy,
                        to: app(),
                        hops: 1,
                    },
                    RelationView {
                        from: NodeView::File {
                            id: "v:2".to_owned()
                        },
                        kind: RelationKindView::OwnedBy,
                        to: app(),
                        hops: 2,
                    },
                ],
                omitted: "0".to_owned(),
            },
            "the candidate's own edge first; its sibling by the same ID scheme"
        );
        assert_eq!(file_id(item.candidate.identity()), "v:1");
        let json = serde_json::to_value(&shown.edges[1]).unwrap_or_default();
        assert_eq!(
            json,
            serde_json::json!({
                "from": {"type": "file", "id": "v:2"},
                "kind": "owned_by",
                "to": {"type": "application", "app": "com.example.app"},
                "hops": 2,
            })
        );
    }

    #[test]
    fn a_bound_never_reads_as_no_more() {
        let mut item = assessed(EntryKind::Directory, AccessState::Readable, Vec::new());
        item.relationships = (1..=20).map(|n| owned_by(n, "com.example.app")).collect();
        let shown = relations(&item);
        assert_eq!(shown.edges.len(), MAX_RELATIONS);
        assert_eq!(shown.omitted, "4");
        assert_eq!(shown.edges[0].hops, 1, "the candidate's own edge survives");
        let none = relations(&assessed(
            EntryKind::Directory,
            AccessState::Readable,
            Vec::new(),
        ));
        assert_eq!(
            none,
            RelationsView {
                edges: Vec::new(),
                omitted: "0".to_owned()
            }
        );
    }
}
