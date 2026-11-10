//! Zgoda na wysyłanie treści wiadomości do zewnętrznych usług tłumaczenia. Stan żyje w backendzie
//! (a nie tylko w interfejsie), więc samo wywołanie polecenia z przeglądarki nie wystarczy do wysłania tekstu.
//! Plik: pierwsza linia `enabled=1|0`, potem po jednej linii z nazwą usługi, na którą wyrażono zgodę.

#[derive(Debug, Clone, PartialEq)]
pub struct Consent {
    pub enabled: bool,
    pub granted: Vec<String>,
}

impl Default for Consent {
    fn default() -> Self {
        Consent { enabled: true, granted: Vec::new() }
    }
}

/// Wczytuje stan; nieznane wpisy są ignorowane. Uszkodzony plik daje bezpieczny stan domyślny (bez zgód).
pub fn parse(text: &str, known: &[&str]) -> Consent {
    let mut c = Consent::default();
    for (i, line) in text.lines().map(str::trim).enumerate() {
        if i == 0 && line.starts_with("enabled=") {
            c.enabled = line != "enabled=0";
        } else if known.contains(&line) && !c.granted.iter().any(|g| g == line) {
            c.granted.push(line.to_string());
        }
    }
    c
}

pub fn render(c: &Consent) -> String {
    let mut out = format!("enabled={}\n", if c.enabled { 1 } else { 0 });
    for g in &c.granted {
        out.push_str(g);
        out.push('\n');
    }
    out
}

/// Czy wolno wysłać tekst do tej usługi.
pub fn check(c: &Consent, provider: &str) -> Result<(), String> {
    if !c.enabled {
        return Err("Tłumaczenie online jest wyłączone w Ustawieniach.".to_string());
    }
    if !c.granted.iter().any(|g| g == provider) {
        return Err("Brak zgody na wysyłanie treści do tej usługi tłumaczenia.".to_string());
    }
    Ok(())
}

/// Zapisuje zgodę na daną usługę (tylko znaną, bez duplikatów).
pub fn grant(c: &mut Consent, provider: &str, known: &[&str]) -> Result<(), String> {
    if !known.contains(&provider) {
        return Err("Nieznana usługa tłumaczenia.".to_string());
    }
    if !c.granted.iter().any(|g| g == provider) {
        c.granted.push(provider.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const K: &[&str] = &["mymemory", "deepl", "google", "microsoft"];

    #[test]
    fn default_has_translation_enabled_but_no_consent() {
        let c = Consent::default();
        assert!(c.enabled && c.granted.is_empty());
        assert!(check(&c, "mymemory").is_err(), "bez zgody nic nie może wyjść");
    }

    #[test]
    fn consent_is_per_provider() {
        let mut c = Consent::default();
        grant(&mut c, "deepl", K).unwrap();
        assert!(check(&c, "deepl").is_ok());
        assert!(check(&c, "google").is_err());
        assert!(grant(&mut c, "evil", K).is_err());
        grant(&mut c, "deepl", K).unwrap();
        assert_eq!(c.granted.len(), 1);
    }

    #[test]
    fn disabling_blocks_everything_even_with_consent() {
        let mut c = Consent::default();
        grant(&mut c, "mymemory", K).unwrap();
        c.enabled = false;
        assert!(check(&c, "mymemory").unwrap_err().contains("wyłączone"));
    }

    #[test]
    fn roundtrip_and_garbage_handling() {
        let mut c = Consent::default();
        grant(&mut c, "google", K).unwrap();
        c.enabled = false;
        assert_eq!(parse(&render(&c), K), c);
        let g = parse("śmieci\n\0\nevil\nmymemory\nmymemory\n", K);
        assert_eq!(g.granted, vec!["mymemory".to_string()]);
        assert!(g.enabled);
        assert_eq!(parse("", K), Consent::default());
    }
}
