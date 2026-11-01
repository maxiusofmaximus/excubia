/// The folders the inbox reads may use: these three, and no other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxFolder {
    Inbox,
    SentItems,
    JunkEmail,
}

impl InboxFolder {
    /// The Graph well-known name of the folder.
    pub fn graph_name(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::SentItems => "sentitems",
            Self::JunkEmail => "junkemail",
        }
    }
}
