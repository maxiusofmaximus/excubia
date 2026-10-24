//! The categories an action falls into. The host assigns them from the
//! structured payload; the agent's own text never takes part.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Read,
    Edit,
    Test,
    Build,
    Command,
    Network,
    Vcs,
    Credentials,
    Money,
    DeleteUserData,
    Publish,
    OutsideWorkspace,
}

impl Category {
    pub const ALL: [Category; 12] = [
        Self::Read,
        Self::Edit,
        Self::Test,
        Self::Build,
        Self::Command,
        Self::Network,
        Self::Vcs,
        Self::Credentials,
        Self::Money,
        Self::DeleteUserData,
        Self::Publish,
        Self::OutsideWorkspace,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Test => "test",
            Self::Build => "build",
            Self::Command => "command",
            Self::Network => "network",
            Self::Vcs => "vcs",
            Self::Credentials => "credentials",
            Self::Money => "money",
            Self::DeleteUserData => "delete_user_data",
            Self::Publish => "publish",
            Self::OutsideWorkspace => "outside_workspace",
        }
    }

    pub fn from_config(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
    }

    /// Actions that always belong to the user: no rule reaches them, and an
    /// allow rule cannot cover them.
    pub fn never_delegate(self) -> bool {
        matches!(
            self,
            Self::Money
                | Self::DeleteUserData
                | Self::Publish
                | Self::Credentials
                | Self::OutsideWorkspace
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Category;

    #[test]
    fn the_never_delegate_list_is_the_rfc_list() {
        let never: Vec<&str> = Category::ALL
            .into_iter()
            .filter(|category| category.never_delegate())
            .map(Category::as_str)
            .collect();
        assert_eq!(
            never,
            [
                "credentials",
                "money",
                "delete_user_data",
                "publish",
                "outside_workspace"
            ]
        );
    }

    #[test]
    fn every_category_round_trips_through_its_config_name() {
        for category in Category::ALL {
            assert_eq!(Category::from_config(category.as_str()), Some(category));
        }
        assert_eq!(Category::from_config("everything"), None);
    }
}
