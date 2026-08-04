use serde::Serialize;

use super::super::serde::{SliceMap, UnitMap};

#[derive_const(Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Security {
    recaptcha_enabled: bool = false,
    allow_user_change_username: bool = true,
    allow_non_privileged_user_edit_public_problem: bool = true,
    allow_owner_manage_problem_permission: bool = true,
    allow_owner_delete_problem: bool = true,
    discussion_default_public: bool = true,
    discussion_reply_default_public: bool = true,
    allow_everyone_create_discussion: bool = true,
    pub max_api_tokens: usize = 20,
}

#[derive_const(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub homepage_user_list: u32 = 10,
    pub homepage_problem_list: u32 = 10,
    problem_set: u32 = 50,
    search_problems_preview: u32 = 7,
    submissions: u32 = 10,
    submission_statistics: u32 = 10,
    user_list: u32 = 30,
    user_audit_logs: u32 = 10,
    discussions: u32 = 10,
    search_discussions_preview: u32 = 7,
    discussion_replies: u32 = 40,
    discussion_replies_head: u32 = 20,
    discussion_replies_more: u32 = 20,
}

#[derive_const(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Misc {
    app_logo_for_theme: UnitMap,
    google_analytics_id: Option<&'static str>,
    plausible_api_endpoint: Option<&'static str>,
    gravatar_cdn: &'static str,
    redirect_legacy_urls: bool,
    render_markdown_in_user_bio: bool,
    discussion_reaction_emojis: &'static [&'static str],
    discussion_reaction_allow_custom_emojis: bool,
    disabled_emoji_in_math: &'static [&'static str],
    lean_versions: &'static SliceMap<&'static str, &'static str>,
}

const impl Default for Misc {
    fn default() -> Self {
        Self {
            app_logo_for_theme: UnitMap {},
            google_analytics_id: option_env!("GOOGLE_ANALYTICS_ID"),
            plausible_api_endpoint: option_env!("PLAUSIBLE_API_ENDPOINT"),
            gravatar_cdn: "https://gravatar.com",
            redirect_legacy_urls: true,
            render_markdown_in_user_bio: true,
            discussion_reaction_emojis: &["👍", "👎", "😄", "😕", "❤", "🤔", "🤣", "🌿", "🍋", "🕊"],
            discussion_reaction_allow_custom_emojis: true,
            disabled_emoji_in_math: &["↔", "↪"],
            lean_versions: SliceMap::from_slice([
                ("4.32.2", "4.32.2 (latest stable)"),
                ("4.33.0-rc2", "4.33.0-rc2 (latest)"),
                ("4.33.0-rc1", "4.33.0-rc1"),
                ("4.32.1", "4.32.1"),
                ("4.32.0", "4.32.0"),
                ("4.32.0-rc1", "4.32.0-rc1"),
                ("4.31.0", "4.31.0"),
                ("4.31.0-rc2", "4.31.0-rc2"),
                ("4.31.0-rc1", "4.31.0-rc1"),
                ("4.30.0", "4.30.0"),
                ("4.30.0-rc2", "4.30.0-rc2"),
                ("4.30.0-rc1", "4.30.0-rc1"),
                ("4.29.1", "4.29.1"),
                ("4.29.0", "4.29.0"),
                ("4.29.0-rc8", "4.29.0-rc8"),
                ("4.29.0-rc7", "4.29.0-rc7"),
                ("4.29.0-rc6", "4.29.0-rc6"),
                ("4.29.0-rc5", "4.29.0-rc5 (w/o mathlib)"),
                ("4.29.0-rc4", "4.29.0-rc4"),
                ("4.29.0-rc3", "4.29.0-rc3"),
                ("4.29.0-rc2", "4.29.0-rc2"),
                ("4.29.0-rc1", "4.29.0-rc1"),
                ("4.28.1", "4.28.1"),
                ("4.28.0", "4.28.0"),
                ("4.28.0-rc1", "4.28.0-rc1"),
                ("4.27.0", "4.27.0"),
                ("4.27.0-rc1", "4.27.0-rc1"),
                ("4.26.0", "4.26.0"),
            ].as_slice()),
        }
    }
}

#[derive_const(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceConfig {
    security: Security,
    pagination: Pagination,
    misc: Misc,
}

impl PreferenceConfig {
    pub const fn make_gravatar_cdn(&mut self) {
        self.misc.gravatar_cdn = "https://cravatar.cn";
    }
}
