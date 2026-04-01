use std::{borrow::Cow, sync::LazyLock};

use lettre::Address;
use pcre2::bytes::{Regex, RegexBuilder};

use super::util::unescape_quoted;

mod lean;
pub use lean::{is_lean_id, is_lean_id_first, is_lean_id_rest};

pub const fn check_username_u8(ch: u8) -> bool {
    matches!(ch, b'#' | b'$' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z')
}

pub fn check_username(username: &str) -> bool {
    3 <= username.len() && username.len() <= 24 && username.bytes().all(check_username_u8)
}

pub const fn is_admin_group(groupname: &str) -> bool {
    groupname.eq_ignore_ascii_case("Lean4OJ.Admin")
}

pub const fn is_system_group(groupname: &str) -> bool {
    if let Some(prefix) = groupname.get(..8) && prefix.eq_ignore_ascii_case("Lean4OJ.") {
        true
    } else {
        false
    }
}

pub const fn check_groupname_u8(ch: u8) -> bool {
    matches!(ch, b' ' | b'#' | b'$' | b'-'..=b':' | b'@'..=b'Z' | b'_' | b'a'..=b'z' | b'~' )
}

pub fn check_groupname(groupname: &str) -> bool {
    !groupname.is_empty() && groupname.len() <= 48 && !is_system_group(groupname) && groupname.bytes().all(check_groupname_u8)
}

pub fn check_uid(uid: &str) -> bool {
    let mut iter = uid.chars();
    let Some(first) = iter.next() else { return false; };
    is_lean_id_first(first) && matches!(
        iter.try_fold(0usize, |len, ch| if is_lean_id_rest(ch) { Some(len + 1) } else { None }),
        Some(2..24),
    )
}

struct EmailRegexs {
    REMOVE_COMMENT: Regex,
    LOCAL_PART: Regex,
    DOMAIN: Regex,
}

mod regex {
    use super::{Cow, Regex, RegexBuilder};

    pub fn create(pattern: &str) -> Regex {
        #[allow(clippy::unwrap_used)]
        RegexBuilder::new()
            .jit_if_available(true)
            .build(pattern)
            .unwrap()
    }

    pub fn remove<'a>(haystack: &'a str, regex: &Regex) -> Cow<'a, str> {
        let mut matches = regex.find_iter(haystack.as_bytes()).filter_map(Result::ok).peekable();

        let Some(first) = matches.next() else { return Cow::Borrowed(haystack) };
        let pre = unsafe { haystack.get_unchecked(..first.start()) };
        let second = matches.peek();
        if second.is_none() {
            if first.end() == haystack.len() {
                return Cow::Borrowed(pre);
            }
            if first.start() == 0 {
                return Cow::Borrowed(unsafe { haystack.get_unchecked(first.end()..) });
            }
        }

        let mut new = String::with_capacity(haystack.len() - (first.end() - first.start()));
        new.push_str(pre);
        let mut last = first.end();

        for mat in matches {
            new.push_str(unsafe { haystack.get_unchecked(last..mat.start()) });
            last = mat.end();
        }

        new.push_str(unsafe { haystack.get_unchecked(last..) });
        Cow::Owned(new)
    }
}

fn remove_comment(mut haystack: &str) -> &str {
    if let Some(s) = haystack.strip_prefix('(') && let Some(i) = s.find(')') {
        haystack = unsafe { s.get_unchecked(i + 1..) };
    }
    if let Some(s) = haystack.strip_suffix(')') && let Some(i) = s.rfind('(') {
        haystack = unsafe { s.get_unchecked(..i) };
    }
    haystack
}

pub fn check_email<'a>(email: &'a str) -> Option<(Cow<'a, str>, &'a str, Address)> {
    static EMAIL_REGEXS: LazyLock<EmailRegexs> = LazyLock::new(|| EmailRegexs {
        REMOVE_COMMENT: regex::create(r"(?:^\([^)]*\))|(?:\([^)]*\))$"),
        LOCAL_PART: regex::create(r#"^(?:[^\s"(),.:;<>@[\\\]]+(?:\.[^\s"(),.:;<>@[\\\]]+)*)$"#),
        DOMAIN: regex::create(r"^(?:(?:\[(?:\d{1,3}\.){3}\d{1,3}])|(?:(?:[\dA-Za-z-]+\.)+[A-Za-z]{2,}))$"),
    });

    if email.is_empty() || email.contains(['«', '»']) || email.bytes().any(|x| x <= 32) {
        return None;
    }

    let (local_part, domain) = email.rsplit_once('@')?;

    let EmailRegexs {
        REMOVE_COMMENT: _,
        LOCAL_PART,
        DOMAIN,
    } = &*EMAIL_REGEXS;

    let domain: &'a str = remove_comment(domain);
    if domain.len() > 254 || domain.split('.').any(|part| part.len() > 63) { return None }

    let Ok(true) = DOMAIN.is_match(domain.as_bytes()) else { return None };

    let local_part: &'a str = remove_comment(local_part);
    if local_part.len() > 64 { return None }

    let address = Address::new(local_part, domain);
    let local_part_1: Cow<'a, str> = if let [b'"', .., b'"'] = *local_part.as_bytes() {
        unescape_quoted(unsafe { local_part.get_unchecked(1..) }).ok()?
    } else {
        match LOCAL_PART.is_match(local_part.as_bytes()) {
            Ok(true) => Cow::Borrowed(local_part),
            _ => return None,
        }
    };

    Some((local_part_1, domain, address.expect("Unexpected email check logic.")))
}

#[cfg(test)]
mod tests {
    use super::{super::logger, Cow, check_email, regex};

    #[test]
    #[cfg_attr(miri, ignore)]
    fn regex() {
        use regex::remove;
        logger::init();

        let re = regex::create("[0-4][5-9][0-5]");
        assert_eq!(remove("1234567", &re), "1234567");
        assert_eq!(remove("1273849", &re), "1849");
        assert_eq!(remove("1627384", &re), "7");
        assert_eq!(remove("114514", &re), "114");
        assert_eq!(remove("145151", &re), "151");
        assert_eq!(remove("1919810", &re), "9810");
        assert_eq!(remove("2021010818", &re), "2021018");
        assert_eq!(remove("2021011832", &re), "2021012");

        assert!(Cow::is_borrowed(&remove("1234567", &re)));
        assert!(Cow::is_owned(&remove("1273849", &re)));
        assert!(Cow::is_owned(&remove("1627384", &re)));
        assert!(Cow::is_owned(&remove("114514", &re)));
        assert!(Cow::is_owned(&remove("145151", &re)));
        assert!(Cow::is_borrowed(&remove("1919810", &re)));
        assert!(Cow::is_owned(&remove("2021010818", &re)));
        assert!(Cow::is_owned(&remove("2021011832", &re)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn email() {
        const VALID: &[&str] = &[
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ@letters-in-local.org",
            "01234567890@numbers-in-local.net",
            "!#$%&'+-/=?^_`{|}~*@other-valid-characters-in-local.net",
            "mixed-1234-in-{+^}-local@sld.net",
            "a@single-character-in-local.org",
            "one-character-third-level@a.example.com",
            "single-character-in-sld@x.org",
            "local@dash-in-sld.com",
            "letters-in-sld@123.com",
            "one-letter-sld@x.org",
            "test@test--1.com",
            "uncommon-tld@sld.museum",
            "uncommon-tld@sld.travel",
            "uncommon-tld@sld.mobi",
            "country-code-tld@sld.uk",
            "country-code-tld@sld.rw",
            "local@sld.newTLD",
            "the-total-length@of-an-entire-address.cannot-be-longer-than-two-hundred-and-fifty-four-characters.and-this-address-is-254-characters-exactly.so-it-should-be-valid.and-im-going-to-add-some-more-words-here.to-increase-the-length-blah-blah-blah-blah-bla.org",
            "the-character-limit@for-each-part.of-the-domain.is-sixty-three-characters.this-is-exactly-sixty-three-characters-so-it-is-valid-blah-blah.com",
            "local@sub.domains.com",
            "backticks`are`legit@test.com",
            "digit-only-domain@123.com",
            "digit-only-domain-with-subdomain@sub.123.com",
            "`a@a.fr",
            "`aa@fr.com",
            "t119037jskc_ihndkdoz@aakctgajathzffcsuqyjhgjuxnuulgnhxtnbquwtgxljfayeestsjdbalthtdd.lgtmsdhywswlameglunsaplsblljavswxrltovagexhtttodqedmicsekvpmpuu.pgjvdmvzyltpixvalfbktnnpjyjqswbfvtpbfsngqtmhgamhrbqqvyvlhqigggv.nxqglspfbwdhtfpibcrccvctmoxuxwlunghhwacjtrclgirrgppvshxvrzkoifl",
            "(comment)myperfect@email.com",
            "myperfect(comment)@email.com",
            "myperfect@email.com(comment)",
            "myperfect@(comment)email.com",
            "(comment)myperfect(comment)@(comment)email.com(comment)",
            "(comment-with-quote)\"comment-with-quote\"(after)@(c)example.com(d)", // newly added
            "\"john..dowe\"@email.com",
            "\"john@smit\"@example.com",
            "\"john\\\"smit\"@example.com",
            "valid-ip@[127.0.0.1]", // newly added
        ];
        const INVALID: &[&str] = &[
            "",
            "@missing-local.org",
            "! #$%`|@invalid-characters-in-local.org",
            "(),:;`|@more-invalid-characters-in-local.org",
            "<>@[]\\`|@even-more-invalid-characters-in-local.org",
            ".local-starts-with-dot@sld.com",
            "local-ends-with-dot.@sld.com",
            "two..consecutive-dots@sld.com",
            "partially.\"quoted\"@sld.com",
            "the-local-part-is-invalid-if-it-is-longer-than-sixty-four-characters@sld.net",
            "missing-sld@.com",
            "invalid-characters-in-sld@! \"#$%(),/;<>_[]`|.org",
            "missing-dot-before-tld@com",
            "missing-tld@sld.",
            "invalid",
            "the-character-limit@for-each-part.of-the-domain.is-sixty-three-characters.this-is-exactly-sixty-four-characters-so-it-is-invalid-blah-blah.com",
            "missing-at-sign.net",
            "unbracketed-IP@127.0.0.1",
            "invalid-ip@127.0.0.1.26",
            "another-invalid-ip@127.0.0.256",
            "IP-and-port@127.0.0.1:25",
            "trailing-dots@test.de.",
            "dot-on-dot-in-domainname@te..st.de",
            "dot-first-in-domain@.test.de",
            "mg@ns.i",
            ".dot-start-and-end.@sil.com",
            "double@a@com",
            "«johnsmith»@example.com",
            "\"john\"smit\"@example.com",
            "\"john\"dowe\"smit\"@example.com", // newly added
            "\"john\"extra@example.com",        // newly added
            "my(comment)perfect@email.com",     // newly added
            "tr119037jskc_ihndkdoz@d.aakctgajathzffcsuqyjhgjuxnuulgnhxtnbquwtgxljfayeestsjdbalthtddy.lgtmsdhywswlameglunsaplsblljavswxrltovagexhtttodqedmicsekvpmpuu.pgjvdmvzyltpixvalfbktnnpjyjqswbfvtpbfsngqtmhgamhrbqqvyvlhqigggv.nxqglspfbwdhtfpibcrccvctmoxuxwlunghhwacjtrclgirrgppvshxvrzkoifl",
        ];

        logger::init();

        for valid in VALID {
            #[allow(clippy::ptr_arg)]
            const fn w(x: &Cow<str>) -> &'static str {
                match x {
                    Cow::Borrowed(_) => "\x1b[33m(borrowed)\x1b[0m",
                    Cow::Owned(_) => "\x1b[36m(owned)\x1b[0m",
                }
            }
            tracing::info!(target: "email-valid", "checking \x1b[32m<valid>\x1b[39m {valid} ...");
            let (local, domain, address) = check_email(valid).unwrap();
            tracing::info!(target: "email-valid", "local = \x1b[32m{local}\x1b[0m {}, domain = \x1b[32m{domain}\x1b[0m, address = {address:?}", w(&local));
        }
        for invalid in INVALID {
            tracing::info!(target: "email-invalid", "checking \x1b[36m<invalid>\x1b[39m {invalid} ...");
            assert!(check_email(invalid).is_none());
        }
    }
}
