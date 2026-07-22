use core::time::Duration;
use std::{sync::LazyLock, time::SystemTime};

use compact_str::CompactString;
use dashmap::{
    DashMap, Entry,
    mapref::{entry_ref::EntryRef, one::RefMut},
};
use hashbrown::DefaultHashBuilder;
use rand::Rng;
use serde::Deserialize;

use crate::libs::{
    constants::{VERIFY_EXPIRE, VERIFY_RESEND},
    util::get_cooldown,
};

#[derive(Copy)]
#[derive_const(Clone, PartialEq, Eq, Deserialize)]
pub enum CodeType {
    ResetPassword,
}

pub struct SendRecord {
    pub time: SystemTime,
    pub code: u32,
}

impl SendRecord {
    #[inline]
    fn new_random(time: SystemTime) -> Self {
        Self { time, code: ((u64::from(rand::rng().next_u32()) * 1_000_000_000) >> 32) as u32 }
    }
}

static MAP: LazyLock<DashMap<CompactString, SendRecord, DefaultHashBuilder>> = LazyLock::new(|| DashMap::with_hasher(DefaultHashBuilder::default()));

pub fn get_code(addr: CompactString, time: SystemTime) -> Result<RefMut<'static, CompactString, SendRecord>, Duration> {
    match MAP.entry(addr) {
        Entry::Occupied(mut e) => {
            let record = e.get_mut();
            get_cooldown(record.time, time, VERIFY_RESEND)?;
            record.time = time;
            Ok(e.into_ref())
        }
        Entry::Vacant(e) => {
            let record = SendRecord::new_random(time);
            Ok(e.insert(record))
        }
    }
}

pub fn email_check(addr: &str, time: SystemTime, code: u32) -> bool {
    let EntryRef::Occupied(e) = MAP.entry_ref(addr) else { return false };
    let record = e.get();
    let ok = record.time.checked_add(VERIFY_EXPIRE).is_none_or(|ddl| time <= ddl) && record.code == code;
    if ok {
        e.remove();
    }
    ok
}

pub fn delete_expired(now: SystemTime) {
    tracing::debug!(target: "expired-token-cleaner", "start clean");

    let cc1 = MAP.len();
    for shard in MAP.shards() {
        if let Some(mut shard) = shard.try_write() {
            shard.retain(|v|
                v.1.time.checked_add(VERIFY_EXPIRE).is_none_or(|ddl| now <= ddl
            ));
        }
    }
    let cc2 = MAP.len();

    tracing::debug!(target: "expired-token-cleaner", "cleaned \x1b[32m{cc1}\x1b[0m -> \x1b[32m{cc2}\x1b[0m");
}

pub fn get_email_content(_type: CodeType, locale: Option<&str>, code: u32) -> (&'static str, String) {
    const PREAMBLE: &str = "<style>code{background-color:rgba(0,0,0,.08);border-radius:3px;display:inline-block;font-family:Menlo,Monaco,Consolas,Courier New,monospace;font-size:.857142857rem;padding:1px 4px}</style>";
    const VERIFY_EXPIRE_MIN: u64 = VERIFY_EXPIRE.as_secs() / 60;

    match locale {
        Some("en_US") => (
            "Your reset password verification code for Lean4OJ",
            format!("{PREAMBLE}<p>Your reset password verification code for Lean4OJ is <code>{code:09}</code>. It will expire in {VERIFY_EXPIRE_MIN} minutes.</p><p>If you are not resetting your password on Lean4OJ, please ignore this email. Your account is still safe.</p>"),
        ),
        Some("ja_JP") => (
            "Lean4OJのパスワードリセット確認コード",
            format!("{PREAMBLE}<p style=\"text-autospace: normal\">Lean4OJのパスワードリセット確認コードは<code>{code:09}</code>です。このコードは{VERIFY_EXPIRE_MIN}分で期限切れになります。</p><p style=\"text-autospace: normal\">Lean4OJでパスワードをリセットしていない場合は、このメールを無視してください。あなたのアカウントはまだ安全です。</p>"),
        ),
        _ => (
            "您在Lean4OJ的密码重置验证码",
            format!("{PREAMBLE}<p style=\"text-autospace: normal\">您在Lean4OJ的密码重置验证码为<code>{code:09}</code>。此验证码在{VERIFY_EXPIRE_MIN}分钟内有效，请尽快完成密码重置。</p><p style=\"text-autospace: normal\">如果您没有尝试在Lean4OJ上进行密码重置，请忽略本邮件。您的账户仍然安全。</p>"),
        ),
    }
}
