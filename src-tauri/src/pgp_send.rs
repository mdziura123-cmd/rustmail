//! Wysyłanie wiadomości podpisanych i zaszyfrowanych OpenPGP (PGP/MIME, RFC 3156).
//!
//! Zasady:
//! - szyfrujemy tylko wtedy, gdy KAŻDY odbiorca (Do, DW, UDW) ma klucz – inaczej błąd z listą adresów,
//!   nigdy cicha wysyłka bez szyfrowania;
//! - wiadomość zawsze jest szyfrowana także do własnego klucza nadawcy (żeby dało się ją przeczytać w Wysłanych);
//! - odbiorcy UDW są „ukryci”: identyfikatory ich kluczy nie trafiają do wiadomości (inni odbiorcy ich nie poznają);
//! - temat i adresy zostają jawne (tak działa PGP/MIME) – interfejs mówi o tym użytkownikowi.
//!
//! Tylko biblioteka standardowa (testowane z prawdziwym gpg).

use crate::compose::{self, OutAttachment, Outgoing};
use crate::pgp::{self, Gpg, Key};

pub struct Options<'a> {
    pub sign: bool,
    pub encrypt: bool,
    pub passphrase: Option<&'a str>,
}

fn bare(addr: &str) -> String {
    pgp::uid_email(addr).unwrap_or_else(|| addr.trim().to_ascii_lowercase())
}

/// Adresy, dla których nie ma działającego klucza do szyfrowania.
pub fn missing_keys(keys: &[Key], addrs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in addrs {
        let e = bare(a);
        if pgp::key_for(keys, &e, false, true).is_none() && !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// Własny klucz nadawcy do podpisu (musi mieć część prywatną).
pub fn signing_key<'a>(keys: &'a [Key], from: &str) -> Result<&'a Key, String> {
    let e = bare(from);
    pgp::key_for(keys, &e, true, false).ok_or_else(|| {
        format!("Nie masz klucza OpenPGP dla adresu {e}. Utwórz go albo zaimportuj w Ustawieniach → Szyfrowanie OpenPGP.")
    })
}

/// Buduje wiadomość podpisaną i/lub zaszyfrowaną. Bez żadnej z opcji – zwykła wiadomość.
pub fn protect(g: &Gpg, m: &Outgoing, nonce: &str, o: &Options) -> Result<Vec<u8>, String> {
    if !o.sign && !o.encrypt {
        return compose::build_message(m, nonce);
    }
    let nonce: String = nonce.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let keys = g.list_keys()?;
    let signer = if o.sign { Some(signing_key(&keys, &m.from)?.fingerprint.clone()) } else { None };
    let (headers, entity) = compose::build_split(m, &nonce)?;

    if !o.encrypt {
        let fpr = signer.as_deref().unwrap_or("");
        let sig = g.sign_detached(&entity, fpr, o.passphrase)?;
        return Ok(pgp::wrap_signed(&headers, &entity, &sig, &nonce));
    }

    let visible: Vec<String> = m.to.iter().chain(m.cc.iter()).cloned().collect();
    let mut missing = missing_keys(&keys, &visible);
    for e in missing_keys(&keys, &m.bcc) {
        if !missing.contains(&e) {
            missing.push(e);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "Brak klucza publicznego OpenPGP dla: {}. Zaimportuj klucz odbiorcy (Ustawienia → Szyfrowanie OpenPGP) albo wyłącz szyfrowanie.",
            missing.join(", ")
        ));
    }
    let own = pgp::key_for(&keys, &bare(&m.from), true, true).ok_or_else(|| {
        format!(
            "Nie masz własnego klucza OpenPGP dla adresu {} – bez niego nie odczytasz tej wiadomości w Wysłanych. Utwórz go w Ustawieniach → Szyfrowanie OpenPGP.",
            bare(&m.from)
        )
    })?;
    let mut recipients: Vec<String> = vec![own.fingerprint.clone()];
    for a in &visible {
        if let Some(k) = pgp::key_for(&keys, &bare(a), false, true) {
            if !recipients.contains(&k.fingerprint) {
                recipients.push(k.fingerprint.clone());
            }
        }
    }
    let mut hidden: Vec<String> = Vec::new();
    for a in &m.bcc {
        if let Some(k) = pgp::key_for(&keys, &bare(a), false, true) {
            if !recipients.contains(&k.fingerprint) && !hidden.contains(&k.fingerprint) {
                hidden.push(k.fingerprint.clone());
            }
        }
    }
    let armored = g.encrypt(&entity, &recipients, &hidden, signer.as_deref().map(|f| (f, o.passphrase)))?;
    Ok(pgp::wrap_encrypted(&headers, &armored, &nonce))
}

/// Własny klucz publiczny nadawcy jako załącznik (żeby odbiorca mógł odpisać zaszyfrowaną wiadomością).
pub fn own_key_attachment(g: &Gpg, from: &str) -> Result<OutAttachment, String> {
    let keys = g.list_keys()?;
    let k = signing_key(&keys, from)?;
    let text = g.export_public(&k.fingerprint)?;
    let short = &k.fingerprint[k.fingerprint.len() - 16..];
    Ok(OutAttachment { name: format!("OpenPGP_0x{short}.asc"), mime: "application/pgp-keys".to_string(), data: text.into_bytes() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(from: &str, to: &[&str], bcc: &[&str]) -> Outgoing {
        Outgoing {
            from: from.into(),
            to: to.iter().map(|s| s.to_string()).collect(),
            cc: vec![],
            bcc: bcc.iter().map(|s| s.to_string()).collect(),
            subject: "Tajne".into(),
            text: "Treść tajna".into(),
            html: "<p>Treść tajna</p>".into(),
            in_reply_to: None,
            references: None,
            attachments: vec![],
        }
    }

    #[test]
    fn missing_key_detection_without_gpg() {
        assert_eq!(missing_keys(&[], &["Jan <JAN@x.pl>".to_string(), "jan@x.pl".to_string()]), vec!["jan@x.pl"]);
    }

    #[test]
    fn send_side_with_real_gpg() {
        let home = std::env::temp_dir().join(format!("rmpgp-send-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let g = Gpg::open(home.clone()).unwrap();
        if g.version().is_err() {
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
        struct C(Gpg);
        impl Drop for C {
            fn drop(&mut self) {
                self.0.stop_agent();
                let _ = std::fs::remove_dir_all(self.0.home());
            }
        }
        let g = C(g);
        let g = &g.0;
        let me = g.generate("Ja", "ja@example.com", "moje-haslo-1").unwrap();
        let anna = g.generate("Anna", "anna@example.com", "haslo-anny-1").unwrap();
        let ukryty = g.generate("U", "ukryty@example.com", "haslo-ukr-12").unwrap();
        let pass = Some("moje-haslo-1");

        // Bez klucza odbiorcy – błąd z adresem, nic nie jest wysyłane bez szyfrowania.
        let e = protect(g, &out("ja@example.com", &["obcy@example.com"], &[]), "n0nce123", &Options { sign: false, encrypt: true, passphrase: None }).unwrap_err();
        assert!(e.contains("obcy@example.com"), "{e}");
        // Podpis bez własnego klucza.
        assert!(protect(g, &out("inny@example.com", &["anna@example.com"], &[]), "n0nce123", &Options { sign: true, encrypt: false, passphrase: None }).is_err());

        // Podpis.
        let signed = protect(g, &out("ja@example.com", &["anna@example.com"], &[]), "n0nce123", &Options { sign: true, encrypt: false, passphrase: pass }).unwrap();
        let st = String::from_utf8_lossy(&signed).to_string();
        assert!(st.contains("multipart/signed; micalg=pgp-sha256"));
        assert!(st.contains("Subject: Tajne\r\n"));
        let body = &signed[pgp::body_start(&signed).unwrap()..];
        let part = pgp::signed_part(body, "rm_sig_n0nce123").unwrap();
        let sig_txt = pgp::extract_armor(&st, "PGP SIGNATURE").unwrap();
        let v = g.verify_detached(&part, sig_txt.as_bytes()).unwrap();
        assert_eq!((v.status.as_str(), v.fingerprint.as_str()), ("good", me.as_str()));

        // Szyfrowanie + podpis, z ukrytym odbiorcą.
        let enc = protect(g, &out("Ja <ja@example.com>", &["anna@example.com"], &["ukryty@example.com"]), "n0nce456", &Options { sign: true, encrypt: true, passphrase: pass }).unwrap();
        let et = String::from_utf8_lossy(&enc).to_string();
        assert!(et.contains("multipart/encrypted;"));
        assert!(!et.contains("Treść tajna") && !et.contains(&compose::base64_encode("Treść tajna".as_bytes())));
        assert!(et.contains("Bcc: ukryty@example.com\r\n"));
        let armored = pgp::extract_armor(&et, "PGP MESSAGE").unwrap();
        let listed = g.run(&["--list-packets"], armored.as_bytes(), std::time::Duration::from_secs(20)).unwrap();
        // Wypis pakietów (stdout) jest taki sam w GnuPG 2.2 i 2.4.
        let dump = String::from_utf8_lossy(&listed.stdout).to_string() + &listed.log;
        let enc_to: Vec<&str> = dump.lines().filter(|l| l.contains("pubkey enc packet")).collect();
        assert_eq!(enc_to.len(), 3, "{dump}");
        assert_eq!(enc_to.iter().filter(|l| l.contains("keyid 0000000000000000")).count(), 1);
        g.forget_passphrases();
        for (who, p) in [(&anna, "haslo-anny-1"), (&ukryty, "haslo-ukr-12"), (&me, "moje-haslo-1")] {
            let d = g.decrypt(armored.as_bytes(), Some(p)).unwrap_or_else(|e| panic!("{who}: {e}"));
            let inner = String::from_utf8(d.plain).unwrap();
            assert!(inner.starts_with("Content-Type: multipart/alternative"));
            assert!(inner.contains(&compose::base64_encode("Treść tajna".as_bytes())));
            assert_eq!(d.signature.unwrap().fingerprint, me);
            g.forget_passphrases();
        }

        // Załącznik z własnym kluczem publicznym.
        let att = own_key_attachment(g, "ja@example.com").unwrap();
        assert_eq!(att.mime, "application/pgp-keys");
        assert!(att.name.ends_with(&format!("{}.asc", &me[24..])));
        assert!(String::from_utf8(att.data).unwrap().contains("BEGIN PGP PUBLIC KEY BLOCK"));
    }
}
