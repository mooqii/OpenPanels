use clap::{ArgAction, Command};
use crate::error::CliError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const COMMAND_CATALOG_VERSION: u32 = 6;
const COMMAND_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandId {
    Catalog(usize),
    InternalStudioServe,
    ParseError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandGroup {
    Version,
    Studio,
    Update,
    Project,
    Panel,
    Canvas,
    Wiki,
    Writing,
    Typesetting,
    Publishing,
    Task,
    Workflow,
    Operation,
    Agent,
    InternalStudioServe,
    ParseError,
}

impl CommandId {
    pub(crate) fn from_intent(intent: &str) -> Option<Self> {
        match intent {
            value if value == INTERNAL_STUDIO_DEFINITION.intent => Some(Self::InternalStudioServe),
            "cli.parse" => Some(Self::ParseError),
            _ => SPECS
                .iter()
                .position(|spec| spec.intent == intent)
                .map(Self::Catalog),
        }
    }

    pub(crate) fn intent(self) -> &'static str {
        match self {
            Self::Catalog(index) => SPECS[index].intent,
            Self::InternalStudioServe => INTERNAL_STUDIO_DEFINITION.intent,
            Self::ParseError => "cli.parse",
        }
    }

    pub(crate) fn registered(intent: &str) -> Self {
        Self::from_intent(intent)
            .filter(|id| matches!(id, Self::Catalog(_)))
            .unwrap_or_else(|| panic!("command is not registered: {intent}"))
    }

    pub(crate) fn group(self) -> CommandGroup {
        match self {
            Self::InternalStudioServe => CommandGroup::InternalStudioServe,
            Self::ParseError => CommandGroup::ParseError,
            Self::Catalog(index) => match SPECS[index].path[0] {
                "version" => CommandGroup::Version,
                "studio" => CommandGroup::Studio,
                "update" => CommandGroup::Update,
                "project" => CommandGroup::Project,
                "panel" => CommandGroup::Panel,
                "canvas" => CommandGroup::Canvas,
                "wiki" => CommandGroup::Wiki,
                "writing" => CommandGroup::Writing,
                "typesetting" => CommandGroup::Typesetting,
                "publishing" => CommandGroup::Publishing,
                "task" => CommandGroup::Task,
                "workflow" => CommandGroup::Workflow,
                "operation" => CommandGroup::Operation,
                "agent" => CommandGroup::Agent,
                path => panic!("unsupported registered command group: {path}"),
            },
        }
    }
}

#[derive(Clone, Copy)]
struct CommandDefinition {
    intent: &'static str,
    path: &'static [&'static str],
    title: &'static str,
    scope: &'static str,
    target_mode: &'static str,
    mutates: bool,
    required_panel_kind: Option<&'static str>,
    audience: CommandAudience,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandAudience {
    Agent,
    Host,
    Protocol,
    Operator,
    #[allow(dead_code)]
    Internal,
}

macro_rules! spec {
    ($intent:literal, [$($path:tt),+], $title:literal, $scope:literal, $target:literal, $mutates:literal) => {
        CommandDefinition { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: None, audience: command_audience!($($path),+) }
    };
    ($intent:literal, [$($path:tt),+], $title:literal, $scope:literal, $target:literal, $mutates:literal, panel=$panel:literal) => {
        CommandDefinition { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: Some($panel), audience: command_audience!($($path),+) }
    };
}

macro_rules! command_audience {
    ("studio", $($rest:literal),*) => {
        CommandAudience::Host
    };
    ("update", $($rest:literal),*) => {
        CommandAudience::Host
    };
    ("version") => {
        CommandAudience::Host
    };
    ("__serve-studio") => {
        CommandAudience::Internal
    };
    ("agent", "bridge", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "target", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "route", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "skill", $($rest:literal),*) => {
        CommandAudience::Agent
    };
    ("agent", $($rest:literal),*) => {
        CommandAudience::Protocol
    };
    ($($rest:literal),*) => {
        CommandAudience::Agent
    };
}

const INTERNAL_STUDIO_DEFINITION: CommandDefinition = CommandDefinition {
    intent: "internal.studio.serve",
    path: &["__serve-studio"],
    title: "Serve Studio internally",
    scope: "internal",
    target_mode: "none",
    mutates: true,
    required_panel_kind: None,
    audience: CommandAudience::Internal,
};
