pub fn is_lean_id_first(ch: char) -> bool {
    if ch.is_alphabetic() { return true; }
    if ch == '_' { return true; }
    if matches!(ch, 'α'..='ϻ') && ch != 'λ' { return true; }
    if matches!(ch, 'Α'..='Ο' | 'Ρ' | 'Τ'..='Ω') { return true; }
    if matches!(ch, 'ἀ'..='῾' | '℀'..='⅏' | '𝒜'..='𝖟') { return true; }
    matches!(ch, 'À'..='ſ') && ch != '×' && ch != '÷'
}

pub fn is_lean_id_rest(ch: char) -> bool {
    if is_lean_id_first(ch) { return true; }
    if ch.is_ascii_digit() { return true; }
    if matches!(ch, '\'' | '!' | '?') { return true; }
    matches!(ch, '₀'..='ₜ' | 'ᵢ'..='ᵪ' | 'ⱼ')
}

pub fn is_lean_id(s: &str) -> bool {
    let mut iter = s.chars();
    let Some(first) = iter.next() else { return false; };
    is_lean_id_first(first) && iter.all(is_lean_id_rest)
}
