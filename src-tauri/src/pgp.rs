//! Szyfrowanie i podpisywanie OpenPGP przez systemowy GnuPG (`gpg`).
//!
//! Dlaczego GnuPG, a nie biblioteka w Ruście: to najstarsza i najlepiej sprawdzona implementacja OpenPGP,
//! jest w każdym Bazzite/Fedorze (korzysta z niej system aktualizacji) i dostaje poprawki bezpieczeństwa
//! razem z systemem. Ten moduł nie implementuje żadnej kryptografii – tylko bezpiecznie woła `gpg`.
//!
//! Izolacja: RustMail ma WŁASNY pęk kluczy w swoim katalogu danych (`…/pl.rustmail.app/gnupg`, prawa 0700),
//! oddzielny od `~/.gnupg` użytkownika. Własny jest też gpg-agent (gniazdo w tym katalogu albo w katalogu
//! sesji), więc inne programy nie korzystają z odblokowanych tu kluczy.
//!
//! Prywatność: GnuPG nie łączy się tu z siecią – wyłączone jest pobieranie kluczy z serwerów i z WKD
//! (`auto-key-locate local`, `no-auto-key-retrieve`). Klucze dodaje tylko użytkownik.
//!
//! Hasło klucza: przekazywane wyłącznie przez standardowe wejście (pierwsza linia, `--passphrase-fd 0`,
//! tryb loopback) – nigdy w argumentach polecenia (te widać na liście procesów) ani w pliku.
//! gpg-agent pamięta je przez 10 minut (najdłużej godzinę), potem trzeba je podać ponownie.
//!
//! Moduł korzysta tylko z biblioteki standardowej, więc jego testy da się uruchomić z prawdziwym gpg.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// Prefiks błędu, po którym interfejs poznaje, że trzeba zapytać o hasło klucza.
pub const ERR_NEED_PASS: &str = "PGP_HASLO:";
/// Prefiks błędu przy złym haśle.
pub const ERR_BAD_PASS: &str = "PGP_ZLE_HASLO:";

const TIMEOUT: Duration = Duration::from_secs(45);
const KEYGEN_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_INPUT: usize = 60 * 1024 * 1024;
const MAX_PASS: usize = 256;

const GPG_CONF: &str = "\
# Plik zarządzany przez RustMail – zmiany zostaną nadpisane.
no-greeting
no-comments
no-emit-version
# Żadnej sieci: klucze dodaje tylko użytkownik.
auto-key-locate local
no-auto-key-retrieve
keyserver-options no-auto-key-retrieve
# Zaufanie ocenia użytkownik (odcisk klucza widać przy każdej wiadomości).
trust-model always
# Bez AEAD/OCB (LibrePGP) – takie wiadomości nie otwierają się w Thunderbirdzie, Proton Mail i innych.
default-preference-list AES256 AES192 AES SHA512 SHA384 SHA256 SHA224 ZLIB BZIP2 ZIP Uncompressed
personal-cipher-preferences AES256 AES192 AES
personal-digest-preferences SHA512 SHA384 SHA256
cert-digest-algo SHA512
";

const AGENT_CONF: &str = "\
# Plik zarządzany przez RustMail – zmiany zostaną nadpisane.
allow-loopback-pinentry
default-cache-ttl 600
max-cache-ttl 3600
";

static HOME: OnceLock<PathBuf> = OnceLock::new();

/// Ustawia katalog pęku kluczy (wywoływane raz przy starcie aplikacji).
pub fn set_home(dir: PathBuf) {
    let _ = HOME.set(dir);
}

/// GnuPG z pękiem kluczy RustMail.
pub fn gpg() -> Result<Gpg, String> {
    let home = HOME.get().ok_or("Pęk kluczy OpenPGP nie jest gotowy.")?;
    Gpg::open(home.clone())
}

pub struct Gpg {
    home: PathBuf,
}

#[derive(Debug, Default)]
pub struct RunOut {
    pub ok: bool,
    pub stdout: Vec<u8>,
    /// Linie statusu GnuPG bez przedrostka „[GNUPG:] ”.
    pub status: Vec<String>,
    /// Komunikaty gpg dla człowieka (do szczegółów błędu).
    pub log: String,
}

impl RunOut {
    fn has(&self, kw: &str) -> bool {
        self.status.iter().any(|l| l == kw || l.starts_with(&format!("{kw} ")))
    }
    fn first(&self, kw: &str) -> Option<Vec<String>> {
        let p = format!("{kw} ");
        self.status
            .iter()
            .find(|l| l.starts_with(&p) || *l == kw)
            .map(|l| l.split(' ').skip(1).map(str::to_string).collect())
    }
    /// Kod błędu GnuPG (dolne 16 bitów) z linii ERROR/FAILURE.
    fn err_codes(&self) -> Vec<u32> {
        self.status
            .iter()
            .filter(|l| l.starts_with("ERROR ") || l.starts_with("FAILURE "))
            .filter_map(|l| l.split(' ').nth(2).and_then(|c| c.parse::<u32>().ok()))
            .map(|c| c & 0xFFFF)
            .collect()
    }
    fn short_log(&self) -> String {
        let lines: Vec<&str> = self.log.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = &lines[lines.len().saturating_sub(3)..];
        tail.join(" / ").chars().take(400).collect()
    }
}

/// Klucz z pęku kluczy.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub fingerprint: String,
    pub key_id: String,
    /// Identyfikatory użytkownika („Jan Kowalski <jan@x.pl>”).
    pub uids: Vec<String>,
    /// Adresy e-mail z identyfikatorów (małymi literami).
    pub emails: Vec<String>,
    pub algo: String,
    pub created: i64,
    /// 0 = bez daty ważności.
    pub expires: i64,
    pub revoked: bool,
    pub expired: bool,
    pub disabled: bool,
    pub can_encrypt: bool,
    pub can_sign: bool,
    pub secret: bool,
}

impl Key {
    pub fn usable(&self) -> bool {
        !self.revoked && !self.expired && !self.disabled
    }
}

/// Wynik sprawdzenia podpisu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SigInfo {
    /// good | bad | expired_key | revoked_key | expired_sig | no_key | error
    pub status: String,
    pub key_id: String,
    /// Odcisk klucza głównego podpisującego (gdy znany).
    pub fingerprint: String,
    /// Identyfikator użytkownika z klucza (gdy klucz jest w pęku).
    pub signer: String,
    /// Czas złożenia podpisu (sekundy).
    pub created: i64,
}

#[derive(Debug, Default)]
pub struct Decrypted {
    pub plain: Vec<u8>,
    /// Prawda, gdy dane były zaszyfrowane (a nie tylko podpisane).
    pub was_encrypted: bool,
    pub signature: Option<SigInfo>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ImportResult {
    pub fingerprints: Vec<String>,
    pub new_keys: u32,
    pub unchanged: u32,
    pub secret: u32,
}

fn find_gpg() -> Result<PathBuf, String> {
    let names = ["gpg", "gpg2"];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }
    for p in ["/usr/bin/gpg", "/usr/bin/gpg2", "/usr/local/bin/gpg"] {
        if Path::new(p).is_file() {
            return Ok(PathBuf::from(p));
        }
    }
    Err("Nie znaleziono programu GnuPG (gpg). W Bazzite jest on częścią systemu – sprawdź polecenie `gpg --version`.".to_string())
}

fn sibling(gpg: &Path, name: &str) -> PathBuf {
    gpg.parent().map(|d| d.join(name)).unwrap_or_else(|| PathBuf::from(name))
}

/// Odcisk klucza: 40 albo 64 znaki szesnastkowe.
pub fn valid_fpr(f: &str) -> bool {
    (f.len() == 40 || f.len() == 64) && f.chars().all(|c| c.is_ascii_hexdigit())
}

fn check_pass(p: &str) -> Result<(), String> {
    if p.len() > MAX_PASS || p.contains('\n') || p.contains('\r') || p.contains('\0') {
        return Err("Niepoprawne hasło klucza.".to_string());
    }
    Ok(())
}

fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("Nie można utworzyć katalogu kluczy: {e}"))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("Nie można ustawić praw katalogu kluczy: {e}"))
}

fn write_conf(path: &Path, content: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::read_to_string(path).map(|c| c == content).unwrap_or(false) {
        return Ok(());
    }
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("Nie można zapisać ustawień GnuPG: {e}"))?;
    f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| format!("Nie można zapisać ustawień GnuPG: {e}"))
}

/// Odczytuje strumień do końca (z limitem rozmiaru, żeby gpg nie zapchał pamięci).
fn drain(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out.len() < MAX_INPUT * 2 {
                        out.extend_from_slice(&buf[..n]);
                    }
                }
            }
        }
        out
    })
}

impl Gpg {
    /// Przygotowuje katalog pęku kluczy i ustawienia (wywołanie wielokrotne jest bezpieczne).
    pub fn open(home: PathBuf) -> Result<Self, String> {
        private_dir(&home)?;
        write_conf(&home.join("gpg.conf"), GPG_CONF)?;
        write_conf(&home.join("gpg-agent.conf"), AGENT_CONF)?;
        Ok(Gpg { home })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    fn base(&self) -> Result<Command, String> {
        let bin = find_gpg()?;
        let mut c = Command::new(bin);
        c.env("GNUPGHOME", &self.home)
            .env("LC_ALL", "C")
            .env("LANGUAGE", "C")
            .env_remove("GPG_AGENT_INFO")
            .arg("--homedir")
            .arg(&self.home)
            .args(["--batch", "--no-tty", "--status-fd", "2", "--with-colons", "--fixed-list-mode"]);
        Ok(c)
    }

    /// Uruchamia gpg z podanymi argumentami; `input` trafia na standardowe wejście.
    pub fn run(&self, args: &[&str], input: &[u8], timeout: Duration) -> Result<RunOut, String> {
        if input.len() > MAX_INPUT {
            return Err("Wiadomość jest zbyt duża do zaszyfrowania lub odszyfrowania.".to_string());
        }
        let mut cmd = self.base()?;
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("Nie można uruchomić GnuPG: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("Brak wejścia GnuPG.")?;
        let data = input.to_vec();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
            // Zamknięcie wejścia sygnalizuje koniec danych.
        });
        let out = drain(child.stdout.take().ok_or("Brak wyjścia GnuPG.")?);
        let err = drain(child.stderr.take().ok_or("Brak wyjścia GnuPG.")?);
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Some(s),
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(15)),
                Err(e) => return Err(format!("Błąd GnuPG: {e}")),
            }
        };
        let _ = writer.join();
        let stdout = out.join().unwrap_or_default();
        let stderr = err.join().unwrap_or_default();
        let Some(status) = status else {
            return Err("GnuPG nie odpowiedział na czas.".to_string());
        };
        let text = String::from_utf8_lossy(&stderr);
        let mut st = Vec::new();
        let mut log = String::new();
        for line in text.lines() {
            if let Some(s) = line.strip_prefix("[GNUPG:] ") {
                st.push(s.to_string());
            } else {
                log.push_str(line);
                log.push('\n');
            }
        }
        Ok(RunOut { ok: status.success(), stdout, status: st, log })
    }

    /// Wersja GnuPG (np. „gpg (GnuPG) 2.4.4”).
    pub fn version(&self) -> Result<String, String> {
        let bin = find_gpg()?;
        let out = Command::new(bin)
            .arg("--version")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Nie można uruchomić GnuPG: {e}"))?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_string())
    }

    /// Zapomina hasła zapamiętane przez gpg-agent.
    pub fn forget_passphrases(&self) {
        if let Ok(bin) = find_gpg() {
            let _ = Command::new(sibling(&bin, "gpgconf"))
                .arg("--homedir")
                .arg(&self.home)
                .args(["--reload", "gpg-agent"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    /// Zatrzymuje gpg-agent tego pęku (przy zamykaniu aplikacji).
    pub fn stop_agent(&self) {
        if let Ok(bin) = find_gpg() {
            let _ = Command::new(sibling(&bin, "gpgconf"))
                .arg("--homedir")
                .arg(&self.home)
                .args(["--kill", "gpg-agent"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    // ---------- Klucze ----------

    pub fn list_keys(&self) -> Result<Vec<Key>, String> {
        let pubs = self.run(&["--list-keys"], b"", TIMEOUT)?;
        let secs = self.run(&["--list-secret-keys"], b"", TIMEOUT)?;
        let secret_fprs: Vec<String> = parse_colons(&String::from_utf8_lossy(&secs.stdout)).into_iter().map(|k| k.fingerprint).collect();
        let mut keys = parse_colons(&String::from_utf8_lossy(&pubs.stdout));
        for k in &mut keys {
            k.secret = secret_fprs.contains(&k.fingerprint);
        }
        Ok(keys)
    }

    pub fn key_by_fpr(&self, fpr: &str) -> Result<Option<Key>, String> {
        Ok(self.list_keys()?.into_iter().find(|k| k.fingerprint.eq_ignore_ascii_case(fpr)))
    }

    /// Tworzy nowy klucz (Ed25519 do podpisu + Curve25519 do szyfrowania, ważny 3 lata).
    pub fn generate(&self, name: &str, email: &str, passphrase: &str) -> Result<String, String> {
        check_pass(passphrase)?;
        if passphrase.chars().count() < 8 {
            return Err("Hasło klucza musi mieć co najmniej 8 znaków.".to_string());
        }
        let name: String = name.chars().filter(|c| !c.is_control() && !"<>()\"".contains(*c)).collect::<String>().trim().chars().take(80).collect();
        let email = email.trim().to_ascii_lowercase();
        if email.is_empty() || email.len() > 254 || !email.contains('@') || email.chars().any(|c| c.is_whitespace() || c.is_control() || "<>()\"".contains(c)) {
            return Err("Niepoprawny adres e-mail klucza.".to_string());
        }
        let uid = if name.is_empty() { format!("<{email}>") } else { format!("{name} <{email}>") };
        let input = format!("{passphrase}\n");
        let out = self.run(
            &["--pinentry-mode", "loopback", "--passphrase-fd", "0", "--quick-generate-key", &uid, "future-default", "default", "3y"],
            input.as_bytes(),
            KEYGEN_TIMEOUT,
        )?;
        match out.first("KEY_CREATED").and_then(|f| f.get(1).cloned()) {
            Some(fpr) if out.ok && valid_fpr(&fpr) => Ok(fpr.to_ascii_uppercase()),
            _ => Err(format!("Nie udało się utworzyć klucza. {}", out.short_log())),
        }
    }

    /// Importuje klucze (publiczne lub prywatne) z tekstu ASCII albo postaci binarnej.
    pub fn import(&self, data: &[u8]) -> Result<ImportResult, String> {
        if data.is_empty() || data.len() > 5 * 1024 * 1024 {
            return Err("Nieprawidłowy plik klucza.".to_string());
        }
        let out = self.run(&["--import-options", "import-clean", "--import"], data, TIMEOUT)?;
        let mut r = ImportResult::default();
        for l in &out.status {
            if let Some(rest) = l.strip_prefix("IMPORT_OK ") {
                let mut it = rest.split(' ');
                let reason: u32 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                if let Some(f) = it.next().filter(|f| valid_fpr(f)) {
                    let f = f.to_ascii_uppercase();
                    if !r.fingerprints.contains(&f) {
                        r.fingerprints.push(f);
                    }
                }
                if reason & 16 != 0 {
                    r.secret += 1;
                }
            }
            if let Some(rest) = l.strip_prefix("IMPORT_RES ") {
                let n: Vec<u32> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
                // count no_user_id imported imported_rsa unchanged …
                r.new_keys = n.get(2).copied().unwrap_or(0);
                r.unchanged = n.get(4).copied().unwrap_or(0);
            }
        }
        if r.fingerprints.is_empty() {
            return Err("W tych danych nie ma klucza OpenPGP, który dałoby się zaimportować.".to_string());
        }
        Ok(r)
    }

    /// Klucz publiczny w postaci tekstowej (do wysłania innym).
    pub fn export_public(&self, fpr: &str) -> Result<String, String> {
        if !valid_fpr(fpr) {
            return Err("Niepoprawny odcisk klucza.".to_string());
        }
        let out = self.run(&["--armor", "--export-options", "export-minimal", "--export", fpr], b"", TIMEOUT)?;
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.ok || !s.contains("BEGIN PGP PUBLIC KEY BLOCK") {
            return Err("Nie udało się wyeksportować klucza.".to_string());
        }
        Ok(s)
    }

    /// Kopia zapasowa klucza prywatnego (zaszyfrowana tym samym hasłem). Wymaga hasła.
    pub fn export_secret(&self, fpr: &str, passphrase: Option<&str>) -> Result<String, String> {
        if !valid_fpr(fpr) {
            return Err("Niepoprawny odcisk klucza.".to_string());
        }
        let (args, input) = pass_args(passphrase)?;
        let mut a: Vec<&str> = args.iter().map(String::as_str).collect();
        a.extend(["--armor", "--export-secret-keys", fpr]);
        let out = self.run(&a, input.as_bytes(), TIMEOUT)?;
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if out.ok && s.contains("BEGIN PGP PRIVATE KEY BLOCK") {
            return Ok(s);
        }
        Err(pass_error(&out, passphrase.is_some()).unwrap_or_else(|| format!("Nie udało się wyeksportować klucza. {}", out.short_log())))
    }

    /// Usuwa klucz (publiczny razem z prywatnym, jeśli jest).
    pub fn delete(&self, fpr: &str) -> Result<(), String> {
        if !valid_fpr(fpr) {
            return Err("Niepoprawny odcisk klucza.".to_string());
        }
        let out = self.run(&["--yes", "--delete-secret-and-public-key", fpr], b"", TIMEOUT)?;
        if !out.ok {
            return Err(format!("Nie udało się usunąć klucza. {}", out.short_log()));
        }
        Ok(())
    }

    // ---------- Szyfrowanie i podpis ----------

    /// Szyfruje dane do podanych odbiorców (odciski). `hidden` – odbiorcy ukryci (UDW): ich identyfikatory
    /// kluczy nie są zapisywane w wiadomości. Opcjonalnie podpisuje kluczem `sign`.
    pub fn encrypt(&self, data: &[u8], recipients: &[String], hidden: &[String], sign: Option<(&str, Option<&str>)>) -> Result<String, String> {
        if recipients.is_empty() && hidden.is_empty() {
            return Err("Brak odbiorców do zaszyfrowania.".to_string());
        }
        let mut args: Vec<String> = Vec::new();
        let mut input = String::new();
        if let Some((fpr, pass)) = sign {
            if !valid_fpr(fpr) {
                return Err("Niepoprawny odcisk klucza.".to_string());
            }
            let (a, i) = pass_args(pass)?;
            args.extend(a);
            input = i;
            args.extend(["--sign".to_string(), "--local-user".to_string(), fpr.to_string(), "--digest-algo".to_string(), "SHA256".to_string()]);
        }
        args.extend(["--armor".to_string(), "--encrypt".to_string(), "--no-encrypt-to".to_string()]);
        for r in recipients {
            if !valid_fpr(r) {
                return Err("Niepoprawny odcisk klucza odbiorcy.".to_string());
            }
            args.push("--recipient".to_string());
            args.push(r.clone());
        }
        for r in hidden {
            if !valid_fpr(r) {
                return Err("Niepoprawny odcisk klucza odbiorcy.".to_string());
            }
            args.push("--hidden-recipient".to_string());
            args.push(r.clone());
        }
        let mut stdin = input.into_bytes();
        stdin.extend_from_slice(data);
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&a, &stdin, TIMEOUT)?;
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if out.ok && s.contains("BEGIN PGP MESSAGE") && out.has("END_ENCRYPTION") {
            return Ok(s);
        }
        if let Some(e) = pass_error(&out, sign.and_then(|s| s.1).is_some()) {
            return Err(e);
        }
        if let Some(f) = out.first("INV_RECP") {
            return Err(format!("Klucza odbiorcy nie można użyć (unieważniony lub wygasły): {}", f.get(1).cloned().unwrap_or_default()));
        }
        Err(format!("Nie udało się zaszyfrować wiadomości. {}", out.short_log()))
    }

    /// Podpis odłączony (PGP/MIME, tryb tekstowy, SHA-256).
    pub fn sign_detached(&self, data: &[u8], fpr: &str, passphrase: Option<&str>) -> Result<String, String> {
        if !valid_fpr(fpr) {
            return Err("Niepoprawny odcisk klucza.".to_string());
        }
        let (mut args, input) = pass_args(passphrase)?;
        args.extend(["--armor", "--detach-sign", "--textmode", "--digest-algo", "SHA256", "--local-user", fpr].map(String::from));
        let mut stdin = input.into_bytes();
        stdin.extend_from_slice(data);
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&a, &stdin, TIMEOUT)?;
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        if out.ok && s.contains("BEGIN PGP SIGNATURE") && out.has("SIG_CREATED") {
            return Ok(s);
        }
        Err(pass_error(&out, passphrase.is_some()).unwrap_or_else(|| format!("Nie udało się podpisać wiadomości. {}", out.short_log())))
    }

    /// Odszyfrowuje wiadomość OpenPGP (albo sprawdza podpis wiadomości podpisanej w treści).
    pub fn decrypt(&self, data: &[u8], passphrase: Option<&str>) -> Result<Decrypted, String> {
        let (mut args, input) = pass_args(passphrase)?;
        args.push("--decrypt".to_string());
        let mut stdin = input.into_bytes();
        stdin.extend_from_slice(data);
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&a, &stdin, TIMEOUT)?;
        let was_encrypted = out.has("BEGIN_DECRYPTION") || out.has("ENC_TO") || out.has("DECRYPTION_FAILED");
        let signature = parse_sig(&out);
        if was_encrypted {
            // Treść oddajemy tylko po pełnym sukcesie z kontrolą integralności – nigdy częściowo odszyfrowaną.
            let ok = out.has("DECRYPTION_OKAY") && !out.has("DECRYPTION_FAILED") && !out.has("BADMDC");
            if !ok {
                if let Some(e) = pass_error(&out, passphrase.is_some()) {
                    return Err(e);
                }
                if out.has("NO_SECKEY") {
                    return Err("Wiadomość nie jest zaszyfrowana do żadnego z Twoich kluczy – nie da się jej odczytać.".to_string());
                }
                return Err(format!("Nie udało się odszyfrować wiadomości (mogła zostać uszkodzona lub zmieniona). {}", out.short_log()));
            }
        } else if signature.is_none() {
            return Err(format!("To nie jest wiadomość OpenPGP. {}", out.short_log()));
        }
        Ok(Decrypted { plain: out.stdout, was_encrypted, signature })
    }

    /// Sprawdza podpis odłączony nad danymi `data`.
    pub fn verify_detached(&self, data: &[u8], signature: &[u8]) -> Result<SigInfo, String> {
        if signature.is_empty() || signature.len() > 256 * 1024 {
            return Err("Nieprawidłowy podpis.".to_string());
        }
        let tmpdir = self.home.join("tmp");
        private_dir(&tmpdir)?;
        let name = format!("sig-{}-{}.asc", std::process::id(), Instant::now().elapsed().as_nanos() ^ (data.len() as u128) ^ nanos());
        let path = tmpdir.join(name);
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .map_err(|e| format!("Nie można sprawdzić podpisu: {e}"))?;
            f.write_all(signature).map_err(|e| e.to_string())?;
        }
        let p = path.to_string_lossy().to_string();
        let res = self.run(&["--verify", &p, "-"], data, TIMEOUT);
        let _ = std::fs::remove_file(&path);
        let out = res?;
        parse_sig(&out).ok_or_else(|| format!("Nie udało się sprawdzić podpisu. {}", out.short_log()))
    }
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Argumenty i początek wejścia dla operacji wymagających hasła klucza.
fn pass_args(passphrase: Option<&str>) -> Result<(Vec<String>, String), String> {
    let mut args = vec!["--pinentry-mode".to_string(), "loopback".to_string()];
    let mut input = String::new();
    if let Some(p) = passphrase {
        check_pass(p)?;
        args.push("--passphrase-fd".to_string());
        args.push("0".to_string());
        input = format!("{p}\n");
    }
    Ok((args, input))
}

/// Rozpoznaje brak hasła i złe hasło (interfejs wtedy pyta użytkownika).
fn pass_error(out: &RunOut, had_pass: bool) -> Option<String> {
    let bad = out.has("BAD_PASSPHRASE") || out.err_codes().contains(&11);
    if had_pass && bad {
        return Some(format!("{ERR_BAD_PASS} Nieprawidłowe hasło klucza."));
    }
    if !had_pass && (out.has("NEED_PASSPHRASE") || out.has("MISSING_PASSPHRASE") || bad) {
        return Some(format!("{ERR_NEED_PASS} Podaj hasło klucza OpenPGP."));
    }
    None
}

fn parse_sig(out: &RunOut) -> Option<SigInfo> {
    let mut s = SigInfo::default();
    for l in &out.status {
        let f: Vec<&str> = l.split(' ').collect();
        let kw = f[0];
        let set = |s: &mut SigInfo, st: &str| {
            if s.status.is_empty() {
                s.status = st.to_string();
                s.key_id = f.get(1).copied().unwrap_or("").to_string();
                s.signer = f.get(2..).map(|x| x.join(" ")).unwrap_or_default();
            }
        };
        match kw {
            "GOODSIG" => set(&mut s, "good"),
            "BADSIG" => set(&mut s, "bad"),
            "EXPSIG" => set(&mut s, "expired_sig"),
            "EXPKEYSIG" => set(&mut s, "expired_key"),
            "REVKEYSIG" => set(&mut s, "revoked_key"),
            "ERRSIG" if s.status.is_empty() => {
                let rc = f.get(6).copied().unwrap_or("");
                s.status = if rc == "9" { "no_key" } else { "error" }.to_string();
                s.key_id = f.get(1).copied().unwrap_or("").to_string();
                if let Some(fp) = f.get(7).filter(|x| valid_fpr(x)) {
                    s.fingerprint = fp.to_ascii_uppercase();
                }
            }
            "VALIDSIG" if s.fingerprint.is_empty() => {
                // VALIDSIG <fpr> <data> <czas> … <odcisk klucza głównego>
                let primary = f.get(10).or(f.get(1)).copied().unwrap_or("");
                if valid_fpr(primary) {
                    s.fingerprint = primary.to_ascii_uppercase();
                }
                s.created = f.get(3).and_then(|x| x.parse().ok()).unwrap_or(0);
            }
            _ => {}
        }
    }
    // „Dobry” podpis bez VALIDSIG nie może się zdarzyć – na wszelki wypadek traktujemy go jako błąd.
    if s.status == "good" && s.fingerprint.is_empty() {
        s.status = "error".to_string();
    }
    if s.status.is_empty() { None } else { Some(s) }
}

/// Odkodowuje pole z `--with-colons` (np. „\x3a” = dwukropek).
fn unescape_colon(s: &str) -> String {
    let mut out: Vec<u8> = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1] == b'x' {
            if let Ok(v) = u8::from_str_radix(&s[i + 2..i + 4], 16) {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Adres e-mail z identyfikatora „Imię <adres>” (albo sam adres).
pub fn uid_email(uid: &str) -> Option<String> {
    let cand = match (uid.rfind('<'), uid.rfind('>')) {
        (Some(l), Some(r)) if l < r => &uid[l + 1..r],
        _ => uid.trim(),
    };
    let c = cand.trim().to_ascii_lowercase();
    if c.contains('@') && !c.contains(' ') && c.len() <= 254 { Some(c) } else { None }
}

fn algo_name(code: &str, curve: &str) -> String {
    match code {
        "1" | "2" | "3" => "RSA".to_string(),
        "17" => "DSA".to_string(),
        "16" | "20" => "ElGamal".to_string(),
        "22" => if curve.is_empty() { "EdDSA".to_string() } else { curve.to_string() },
        "18" | "19" => if curve.is_empty() { "ECC".to_string() } else { curve.to_string() },
        _ => format!("algorytm {code}"),
    }
}

/// Parsuje wynik `--list-keys --with-colons`.
pub fn parse_colons(s: &str) -> Vec<Key> {
    let mut keys: Vec<Key> = Vec::new();
    let mut in_sub = false;
    for line in s.lines() {
        let f: Vec<&str> = line.split(':').collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        match get(0) {
            "pub" | "sec" => {
                in_sub = false;
                let validity = get(1);
                let caps = get(11);
                keys.push(Key {
                    fingerprint: String::new(),
                    key_id: get(4).to_string(),
                    uids: vec![],
                    emails: vec![],
                    algo: algo_name(get(3), get(16)),
                    created: get(5).parse().unwrap_or(0),
                    expires: get(6).parse().unwrap_or(0),
                    revoked: validity == "r",
                    expired: validity == "e",
                    disabled: caps.contains('D'),
                    can_encrypt: caps.contains('E'),
                    can_sign: caps.contains('S'),
                    secret: get(0) == "sec",
                });
            }
            "sub" | "ssb" => in_sub = true,
            "fpr" if !in_sub => {
                if let Some(k) = keys.last_mut() {
                    if k.fingerprint.is_empty() {
                        k.fingerprint = get(9).to_ascii_uppercase();
                    }
                }
            }
            "uid" => {
                if let Some(k) = keys.last_mut() {
                    let validity = get(1);
                    if validity == "r" {
                        continue;
                    }
                    let uid = unescape_colon(get(9));
                    if let Some(e) = uid_email(&uid) {
                        if !k.emails.contains(&e) {
                            k.emails.push(e);
                        }
                    }
                    k.uids.push(uid.chars().filter(|c| !c.is_control()).take(200).collect());
                }
            }
            _ => {}
        }
    }
    keys.retain(|k| valid_fpr(&k.fingerprint));
    keys
}

/// Najlepszy klucz dla adresu: działający, z wymaganą możliwością, najnowszy.
pub fn key_for<'a>(keys: &'a [Key], email: &str, need_secret: bool, encrypt: bool) -> Option<&'a Key> {
    let e = email.trim().to_ascii_lowercase();
    keys.iter()
        .filter(|k| k.usable() && k.emails.contains(&e) && (!need_secret || k.secret) && if encrypt { k.can_encrypt } else { k.can_sign })
        .max_by_key(|k| k.created)
}

// ---------- PGP/MIME (RFC 3156) ----------

/// Zamienia zakończenia linii na CRLF (postać kanoniczna MIME).
pub fn to_crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 40);
    let mut prev = 0u8;
    for &b in data {
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        prev = b;
    }
    out
}

/// Wiadomość podpisana: `multipart/signed`. `headers` to nagłówki główne (bez Content-Type), zakończone CRLF.
pub fn wrap_signed(headers: &str, entity: &[u8], signature: &str, nonce: &str) -> Vec<u8> {
    let b = format!("rm_sig_{nonce}");
    let mut out = Vec::new();
    out.extend_from_slice(headers.as_bytes());
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/signed; micalg=pgp-sha256;\r\n protocol=\"application/pgp-signature\"; boundary=\"{b}\"\r\n\r\n\
             This is an OpenPGP/MIME signed message (RFC 4880 and 3156)\r\n--{b}\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(entity);
    out.extend_from_slice(
        format!(
            "\r\n--{b}\r\nContent-Type: application/pgp-signature; name=\"signature.asc\"\r\n\
             Content-Description: OpenPGP digital signature\r\nContent-Disposition: attachment; filename=\"signature.asc\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(&to_crlf(signature.trim_end().as_bytes()));
    out.extend_from_slice(format!("\r\n\r\n--{b}--\r\n").as_bytes());
    out
}

/// Wiadomość zaszyfrowana: `multipart/encrypted`.
pub fn wrap_encrypted(headers: &str, armored: &str, nonce: &str) -> Vec<u8> {
    let b = format!("rm_enc_{nonce}");
    let mut out = Vec::new();
    out.extend_from_slice(headers.as_bytes());
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/encrypted;\r\n protocol=\"application/pgp-encrypted\"; boundary=\"{b}\"\r\n\r\n\
             This is an OpenPGP/MIME encrypted message (RFC 4880 and 3156)\r\n\
             --{b}\r\nContent-Type: application/pgp-encrypted\r\nContent-Description: PGP/MIME version identification\r\n\r\nVersion: 1\r\n\r\n\
             --{b}\r\nContent-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
             Content-Description: OpenPGP encrypted message\r\nContent-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(&to_crlf(armored.trim_end().as_bytes()));
    out.extend_from_slice(format!("\r\n\r\n--{b}--\r\n").as_bytes());
    out
}

/// Pozycja końca bloku nagłówków (pierwsza pusta linia) – zwraca początek treści.
pub fn body_start(raw: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\n' {
            if raw.get(i + 1) == Some(&b'\n') {
                return Some(i + 2);
            }
            if raw.get(i + 1) == Some(&b'\r') && raw.get(i + 2) == Some(&b'\n') {
                return Some(i + 3);
            }
        }
        i += 1;
    }
    None
}

/// Wycina z treści `multipart/signed` (body = treść po nagłówkach) dokładne bajty pierwszej części –
/// tej, którą obejmuje podpis – w postaci kanonicznej CRLF.
pub fn signed_part(body: &[u8], boundary: &str) -> Option<Vec<u8>> {
    if boundary.is_empty() || boundary.len() > 200 {
        return None;
    }
    let delim = format!("--{boundary}");
    let d = delim.as_bytes();
    // Początek: linia „--granica” (na początku treści albo po znaku nowej linii).
    let mut start = None;
    let mut i = 0;
    while i + d.len() <= body.len() {
        if (i == 0 || body[i - 1] == b'\n') && &body[i..i + d.len()] == d {
            // Reszta linii (dopuszczalne białe znaki) do końca linii.
            let mut j = i + d.len();
            while j < body.len() && (body[j] == b' ' || body[j] == b'\t') {
                j += 1;
            }
            if body.get(j) == Some(&b'\r') {
                j += 1;
            }
            if body.get(j) == Some(&b'\n') {
                start = Some(j + 1);
                break;
            }
        }
        i += 1;
    }
    let start = start?;
    // Koniec: następna linia zaczynająca się od „--granica”; podpisane dane kończą się przed CRLF poprzedzającym tę linię.
    let mut k = start;
    while k + d.len() <= body.len() {
        if body[k - 1] == b'\n' && &body[k..k + d.len()] == d {
            let mut end = k - 1; // pozycja '\n'
            if end > start && body[end - 1] == b'\r' {
                end -= 1;
            }
            if end < start {
                return None;
            }
            return Some(to_crlf(&body[start..end]));
        }
        k += 1;
    }
    None
}

/// Wycina blok ASCII Armor (np. „PGP MESSAGE” albo „PGP SIGNED MESSAGE” razem z podpisem) z tekstu.
pub fn extract_armor(text: &str, kind: &str) -> Option<String> {
    let begin = format!("-----BEGIN {kind}-----");
    let end_kw = if kind == "PGP SIGNED MESSAGE" { "-----END PGP SIGNATURE-----".to_string() } else { format!("-----END {kind}-----") };
    let s = text.find(&begin)?;
    let rest = &text[s..];
    let e = rest.find(&end_kw)? + end_kw.len();
    Some(rest[..e].replace("\r\n", "\n").replace('\u{a0}', " ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLONS: &str = "tru::1:1795363698:0:3:1:5
pub:-:255:22:4FAA63C6A230EF96:1795363698:1889971698::-:::scESC::::::23::0:
fpr:::::::::3A8D9FD552BB6D205B5551A54FAA63C6A230EF96:
uid:-::::1795363698::0D5F::Jan Kowalski <Jan@X.pl>::::::::::0:
uid:r::::1795363698::0D5E::Stary <stary@x.pl>::::::::::0:
uid:-::::1795363698::0D5D::Dwukropek\\x3a test <d@x.pl>::::::::::0:
sub:-:255:18:A70958767A4C51D9:1795363698:1889971698:::::e::::::23:
fpr:::::::::2EE0B7A35362EA9615C8BCAFA70958767A4C51D9:
pub:e:3072:1:1111111111111111:1500000000:1600000000::-:::sc::::::::0:
fpr:::::::::1111111111111111111111111111111111111111:
uid:e::::1500000000::AAAA::Old <old@x.pl>::::::::::0:
";

    #[test]
    fn colons_are_parsed() {
        let k = parse_colons(COLONS);
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].fingerprint, "3A8D9FD552BB6D205B5551A54FAA63C6A230EF96");
        assert_eq!(k[0].emails, vec!["jan@x.pl", "d@x.pl"]);
        assert_eq!(k[0].uids[1], "Dwukropek: test <d@x.pl>");
        assert!(k[0].can_encrypt && k[0].can_sign && k[0].usable());
        assert_eq!(k[0].algo, "EdDSA");
        assert!(k[1].expired && !k[1].usable() && !k[1].can_encrypt);
        assert!(key_for(&k, "JAN@x.pl", false, true).is_some());
        assert!(key_for(&k, "jan@x.pl", true, false).is_none());
        assert!(key_for(&k, "old@x.pl", false, false).is_none());
        assert!(key_for(&k, "stary@x.pl", false, true).is_none());
    }

    #[test]
    fn uid_emails() {
        assert_eq!(uid_email("Jan <JAN@x.pl>").as_deref(), Some("jan@x.pl"));
        assert_eq!(uid_email("jan@x.pl").as_deref(), Some("jan@x.pl"));
        assert_eq!(uid_email("Jan Kowalski"), None);
    }

    #[test]
    fn crlf_and_body_start() {
        assert_eq!(to_crlf(b"a\nb\r\nc\n"), b"a\r\nb\r\nc\r\n");
        assert_eq!(body_start(b"A: b\r\nC: d\r\n\r\nBODY"), Some(14));
        assert_eq!(body_start(b"A: b\n\nBODY"), Some(6));
        assert_eq!(body_start(b"A: b\r\n"), None);
    }

    #[test]
    fn signed_part_is_exact() {
        let entity = b"Content-Type: text/plain\r\n\r\nWitaj\r\n";
        let msg = wrap_signed("From: a@x.pl\r\n", entity, "-----BEGIN PGP SIGNATURE-----\nX\n-----END PGP SIGNATURE-----\n", "abc12345");
        let body = &msg[body_start(&msg).unwrap()..];
        assert_eq!(signed_part(body, "rm_sig_abc12345").unwrap(), entity.to_vec());
        // Ta sama wiadomość zapisana z LF (np. po stronie odbiorcy) daje te same bajty po kanonizacji.
        let lf: Vec<u8> = String::from_utf8_lossy(body).replace("\r\n", "\n").into_bytes();
        assert_eq!(signed_part(&lf, "rm_sig_abc12345").unwrap(), entity.to_vec());
        assert!(signed_part(body, "inna").is_none());
    }

    #[test]
    fn encrypted_wrapper_shape() {
        let m = String::from_utf8(wrap_encrypted("From: a@x.pl\r\n", "-----BEGIN PGP MESSAGE-----\nQQ\n-----END PGP MESSAGE-----\n", "n1234567")).unwrap();
        assert!(m.contains("multipart/encrypted;\r\n protocol=\"application/pgp-encrypted\"; boundary=\"rm_enc_n1234567\""));
        assert!(m.contains("Version: 1\r\n"));
        assert!(m.contains("-----BEGIN PGP MESSAGE-----\r\nQQ\r\n"));
        assert!(m.trim_end().ends_with("--rm_enc_n1234567--"));
        assert!(!m.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn armor_extraction() {
        let t = "hej\r\n-----BEGIN PGP MESSAGE-----\r\nabc\r\n-----END PGP MESSAGE-----\r\nstopka";
        assert_eq!(extract_armor(t, "PGP MESSAGE").unwrap(), "-----BEGIN PGP MESSAGE-----\nabc\n-----END PGP MESSAGE-----");
        let s = "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\ntxt\n-----BEGIN PGP SIGNATURE-----\nx\n-----END PGP SIGNATURE-----\n";
        assert!(extract_armor(s, "PGP SIGNED MESSAGE").unwrap().ends_with("-----END PGP SIGNATURE-----"));
        assert!(extract_armor("nic", "PGP MESSAGE").is_none());
    }

    #[test]
    fn passphrase_cannot_inject_lines() {
        assert!(pass_args(Some("a\nb")).is_err());
        assert!(pass_args(Some("dobre haslo")).is_ok());
        assert_eq!(pass_args(None).unwrap().1, "");
    }

    #[test]
    fn sig_status_parsing() {
        let out = RunOut {
            status: vec![
                "NEWSIG".into(),
                "GOODSIG 4FAA63C6A230EF96 Jan <jan@x.pl>".into(),
                "VALIDSIG 3A8D9FD552BB6D205B5551A54FAA63C6A230EF96 2026-11-22 1795363712 0 4 0 22 8 01 3A8D9FD552BB6D205B5551A54FAA63C6A230EF96".into(),
            ],
            ..Default::default()
        };
        let s = parse_sig(&out).unwrap();
        assert_eq!(s.status, "good");
        assert_eq!(s.signer, "Jan <jan@x.pl>");
        assert_eq!(s.fingerprint, "3A8D9FD552BB6D205B5551A54FAA63C6A230EF96");
        let nokey = RunOut { status: vec!["ERRSIG 4FAA63C6A230EF96 22 8 01 1795363712 9 3A8D9FD552BB6D205B5551A54FAA63C6A230EF96".into()], ..Default::default() };
        assert_eq!(parse_sig(&nokey).unwrap().status, "no_key");
        let fake = RunOut { status: vec!["GOODSIG 1 x".into()], ..Default::default() };
        assert_eq!(parse_sig(&fake).unwrap().status, "error");
    }

    // ---------- Testy z prawdziwym GnuPG (pomijane, gdy gpg nie jest zainstalowany) ----------

    fn temp_home(tag: &str) -> Option<PathBuf> {
        find_gpg().ok()?;
        // Krótka ścieżka: gniazdo agenta ma limit długości.
        let dir = std::env::temp_dir().join(format!("rmpgp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Some(dir)
    }

    struct Cleanup(Gpg);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.0.stop_agent();
            let _ = std::fs::remove_dir_all(self.0.home());
        }
    }

    #[test]
    fn gpg_full_round_trip() {
        let Some(home) = temp_home("rt") else { return };
        let g = Cleanup(Gpg::open(home).unwrap());
        let g = &g.0;
        assert!(g.version().unwrap().contains("GnuPG"));
        let alice = g.generate("Alicja Nowak", "alicja@example.com", "haslo-alicji-1").unwrap();
        let bob = g.generate("Bob", "bob@example.com", "haslo-boba-22").unwrap();
        let keys = g.list_keys().unwrap();
        let a = key_for(&keys, "alicja@example.com", true, false).unwrap();
        assert_eq!(a.fingerprint, alice);
        assert!(a.secret && a.can_encrypt && a.can_sign);
        assert!(g.generate("X", "x@example.com", "krotkie").is_err());

        // Szyfrowanie + podpis, odszyfrowanie z hasłem.
        let pt = "Zażółć gęślą jaźń\r\n".repeat(50);
        let ct = g.encrypt(pt.as_bytes(), &[bob.clone(), alice.clone()], &[], Some((&alice, Some("haslo-alicji-1")))).unwrap();
        assert!(ct.starts_with("-----BEGIN PGP MESSAGE-----"));
        g.forget_passphrases();
        let need = g.decrypt(ct.as_bytes(), None).unwrap_err();
        assert!(need.starts_with(ERR_NEED_PASS), "{need}");
        let bad = g.decrypt(ct.as_bytes(), Some("zle-haslo-xx")).unwrap_err();
        assert!(bad.starts_with(ERR_BAD_PASS) || bad.contains("nie jest zaszyfrowana"), "{bad}");
        g.forget_passphrases();
        let d = g.decrypt(ct.as_bytes(), Some("haslo-boba-22")).unwrap();
        assert!(d.was_encrypted);
        assert_eq!(String::from_utf8(d.plain).unwrap(), pt);
        let s = d.signature.unwrap();
        assert_eq!(s.status, "good");
        assert_eq!(s.fingerprint, alice);
        // Hasło zapamiętane przez agenta – drugi raz bez pytania.
        let only_bob = g.encrypt(b"dla Boba", &[bob.clone()], &[], None).unwrap();
        assert_eq!(g.decrypt(only_bob.as_bytes(), None).unwrap().plain, b"dla Boba");

        // Ukryty odbiorca: w wiadomości nie ma identyfikatora jego klucza.
        let hidden = g.encrypt(b"tajne", &[alice.clone()], &[bob.clone()], None).unwrap();
        let listed = g.run(&["--list-packets"], hidden.as_bytes(), TIMEOUT).unwrap();
        // Wypis pakietów (stdout) jest taki sam w GnuPG 2.2 i 2.4.
        let dump = String::from_utf8_lossy(&listed.stdout).to_string() + &listed.log;
        let enc_to: Vec<&str> = dump.lines().filter(|l| l.contains("pubkey enc packet")).collect();
        assert_eq!(enc_to.len(), 2, "{dump}");
        assert_eq!(enc_to.iter().filter(|l| l.contains("keyid 0000000000000000")).count(), 1);

        // Podpis odłączony PGP/MIME, sprawdzenie i wykrycie zmiany treści.
        let entity = b"Content-Type: text/plain; charset=UTF-8\r\n\r\nWitaj Bob\r\n";
        let sig = g.sign_detached(entity, &alice, Some("haslo-alicji-1")).unwrap();
        let msg = wrap_signed("From: alicja@example.com\r\n", entity, &sig, "abcdefgh");
        let body = &msg[body_start(&msg).unwrap()..];
        let signed = signed_part(body, "rm_sig_abcdefgh").unwrap();
        let v = g.verify_detached(&signed, sig.as_bytes()).unwrap();
        assert_eq!(v.status, "good");
        assert_eq!(v.fingerprint, alice);
        let mut tampered = signed.clone();
        let n = tampered.len() - 4;
        tampered[n] = b'X';
        assert_eq!(g.verify_detached(&tampered, sig.as_bytes()).unwrap().status, "bad");

        // Eksport/import i usuwanie: inny pęk kluczy nie zna klucza, dopóki go nie zaimportuje.
        let pubkey = g.export_public(&alice).unwrap();
        let Some(home2) = temp_home("rt2") else { return };
        let g2 = Cleanup(Gpg::open(home2).unwrap());
        let g2 = &g2.0;
        assert_eq!(g2.verify_detached(&signed, sig.as_bytes()).unwrap().status, "no_key");
        let imp = g2.import(pubkey.as_bytes()).unwrap();
        assert_eq!(imp.fingerprints, vec![alice.clone()]);
        assert_eq!(imp.secret, 0);
        assert_eq!(g2.verify_detached(&signed, sig.as_bytes()).unwrap().status, "good");
        assert!(g2.import(b"to nie klucz").is_err());
        // Kopia klucza prywatnego wymaga hasła; po imporcie da się nią odszyfrować.
        g.forget_passphrases();
        assert!(g.export_secret(&bob, None).unwrap_err().starts_with(ERR_NEED_PASS));
        let bob_secret = g.export_secret(&bob, Some("haslo-boba-22")).unwrap();
        let imp2 = g2.import(bob_secret.as_bytes()).unwrap();
        assert!(imp2.secret > 0);
        assert_eq!(String::from_utf8(g2.decrypt(ct.as_bytes(), Some("haslo-boba-22")).unwrap().plain).unwrap(), pt);
        g2.delete(&alice).unwrap();
        assert!(g2.key_by_fpr(&alice).unwrap().is_none());

        // Wiadomość podpisana w treści (clearsign) – sprawdzana przez decrypt.
        let clear = g.run(&["--pinentry-mode", "loopback", "--passphrase-fd", "0", "--clearsign", "--local-user", &alice], b"haslo-alicji-1\nCzesc\n", TIMEOUT).unwrap();
        let d2 = g.decrypt(&clear.stdout, None).unwrap();
        assert!(!d2.was_encrypted);
        assert_eq!(d2.signature.unwrap().status, "good");
        assert!(g.decrypt(b"zwykly tekst", None).is_err());
    }
}
