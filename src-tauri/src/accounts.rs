//! Rejestr kont Gmail (czysta logika). Zapisywany w pęku kluczy systemu jako tekst: jedna linia na konto,
//! pola rozdzielone tabulatorem (adres, token odświeżania, przyznany zakres). Żadne pole nie zawiera tabulatora
//! ani końca linii, więc format jest jednoznaczny.

pub const MAX_ACCOUNTS: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub email: String,
    pub refresh: String,
    pub scope: String,
}

fn clean_field(s: &str) -> bool {
    !s.contains(['\t', '\n', '\r', '\0'])
}

pub fn parse(s: &str) -> Vec<Account> {
    s.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let (email, refresh, scope) = (it.next()?, it.next()?, it.next()?);
            if it.next().is_some() || email.is_empty() || refresh.is_empty() {
                return None;
            }
            Some(Account { email: email.to_string(), refresh: refresh.to_string(), scope: scope.to_string() })
        })
        .collect()
}

pub fn serialize(v: &[Account]) -> String {
    v.iter().map(|a| format!("{}\t{}\t{}", a.email, a.refresh, a.scope)).collect::<Vec<_>>().join("\n")
}

/// Dodaje konto albo odświeża istniejące (adres porównywany bez względu na wielkość liter).
pub fn upsert(v: &mut Vec<Account>, a: Account) -> Result<(), String> {
    if a.email.is_empty() || a.refresh.is_empty() || !clean_field(&a.email) || !clean_field(&a.refresh) || !clean_field(&a.scope) {
        return Err("Niepoprawne dane konta.".to_string());
    }
    if let Some(existing) = v.iter_mut().find(|x| x.email.eq_ignore_ascii_case(&a.email)) {
        *existing = a;
        return Ok(());
    }
    if v.len() >= MAX_ACCOUNTS {
        return Err(format!("Można dodać najwyżej {MAX_ACCOUNTS} kont."));
    }
    v.push(a);
    Ok(())
}

pub fn remove(v: &mut Vec<Account>, email: &str) -> Option<Account> {
    let i = v.iter().position(|x| x.email.eq_ignore_ascii_case(email))?;
    Some(v.remove(i))
}

pub fn find<'a>(v: &'a [Account], email: &str) -> Option<&'a Account> {
    v.iter().find(|x| x.email.eq_ignore_ascii_case(email))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(e: &str, r: &str) -> Account {
        Account { email: e.to_string(), refresh: r.to_string(), scope: "https://x/modify other".to_string() }
    }

    #[test]
    fn roundtrip() {
        let v = vec![acc("a@gmail.com", "tok1"), acc("b@gmail.com", "tok/2_x-y")];
        assert_eq!(parse(&serialize(&v)), v);
        assert!(parse("").is_empty());
    }

    #[test]
    fn parse_skips_broken_lines() {
        let s = "a@x\tt\ts\nbroken line\n\tt\ts\nb@x\t\ts\nc@x\tt\ts\textra\nd@x\tt\t\n";
        let v = parse(s);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].email, "a@x");
        assert_eq!(v[1].email, "d@x");
        assert_eq!(v[1].scope, "");
    }

    #[test]
    fn upsert_replaces_case_insensitively() {
        let mut v = vec![acc("A@Gmail.com", "old")];
        upsert(&mut v, acc("a@gmail.com", "new")).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].refresh, "new");
    }

    #[test]
    fn upsert_rejects_bad_data_and_limit() {
        let mut v = Vec::new();
        assert!(upsert(&mut v, acc("a\tb@x", "t")).is_err());
        assert!(upsert(&mut v, acc("a@x", "t\nu")).is_err());
        assert!(upsert(&mut v, acc("", "t")).is_err());
        for i in 0..MAX_ACCOUNTS {
            upsert(&mut v, acc(&format!("u{i}@x"), "t")).unwrap();
        }
        assert!(upsert(&mut v, acc("extra@x", "t")).is_err());
        assert!(upsert(&mut v, acc("u0@x", "t2")).is_ok());
    }

    #[test]
    fn remove_and_find() {
        let mut v = vec![acc("a@x", "1"), acc("b@x", "2")];
        assert_eq!(find(&v, "B@X").unwrap().refresh, "2");
        assert_eq!(remove(&mut v, "a@X").unwrap().refresh, "1");
        assert!(remove(&mut v, "a@x").is_none());
        assert_eq!(v.len(), 1);
    }
}
