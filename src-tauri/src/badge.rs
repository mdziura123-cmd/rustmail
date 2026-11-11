//! Znacznik nowych wiadomości: czerwona kropka na ikonie zasobnika i liczba na ikonie aplikacji w pasku
//! (przez interfejs „Unity Launcher”, który obsługuje m.in. KDE Plasma). Część rysująca i budująca
//! argumenty jest czysta, więc testowana osobno.

/// Rysuje na obrazie RGBA czerwoną kropkę z białą obwódką w prawym górnym rogu.
pub fn draw_dot(rgba: &mut [u8], w: usize, h: usize) {
    if w < 8 || h < 8 || rgba.len() < w * h * 4 {
        return;
    }
    let r = (w.min(h) as f32) * 0.24;
    let cx = w as f32 - r - 1.0;
    let cy = r + 1.0;
    let ring = r;
    let inner = r * 0.74;
    for y in 0..h {
        for x in 0..w {
            let d = (((x as f32 + 0.5) - cx).powi(2) + ((y as f32 + 0.5) - cy).powi(2)).sqrt();
            let outer_cov = (ring + 0.5 - d).clamp(0.0, 1.0);
            if outer_cov <= 0.0 {
                continue;
            }
            let inner_cov = (inner + 0.5 - d).clamp(0.0, 1.0);
            // kolor: biały pierścień, czerwone wnętrze
            let col = [
                255.0,
                255.0 * (1.0 - inner_cov) + 59.0 * inner_cov,
                255.0 * (1.0 - inner_cov) + 48.0 * inner_cov,
            ];
            let i = (y * w + x) * 4;
            let a0 = rgba[i + 3] as f32 / 255.0;
            let a = outer_cov + a0 * (1.0 - outer_cov);
            for k in 0..3 {
                let c0 = rgba[i + k] as f32;
                rgba[i + k] = if a > 0.0 { ((col[k] * outer_cov + c0 * a0 * (1.0 - outer_cov)) / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
            rgba[i + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Argumenty dla `gdbus emit`, które ustawiają licznik na ikonie aplikacji (0 = ukryj).
pub fn unity_args(desktop_id: &str, count: u32) -> Vec<String> {
    vec![
        "emit".into(),
        "--session".into(),
        "--object-path".into(),
        "/com/canonical/unity/launcherentry/1".into(),
        "--signal".into(),
        "com.canonical.Unity.LauncherEntry.Update".into(),
        format!("'application://{desktop_id}.desktop'"),
        format!("{{'count': <int64 {count}>, 'count-visible': <{}>}}", count > 0),
    ]
}

/// Opis do podpowiedzi ikony w zasobniku.
pub fn tooltip(count: u32) -> String {
    match count {
        0 => "RustMail".to_string(),
        1 => "RustMail – 1 nieprzeczytana wiadomość".to_string(),
        n => format!("RustMail – nieprzeczytane wiadomości: {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_red_in_corner_and_leaves_rest() {
        let (w, h) = (32usize, 32usize);
        let mut img = vec![0u8; w * h * 4];
        for px in img.chunks_mut(4) {
            px.copy_from_slice(&[10, 20, 30, 255]);
        }
        draw_dot(&mut img, w, h);
        // lewy dolny róg bez zmian
        let i = (28 * w + 3) * 4;
        assert_eq!(&img[i..i + 4], &[10, 20, 30, 255]);
        // środek kropki jest czerwony
        let r = 32.0f32 * 0.24;
        let (cx, cy) = ((w as f32 - r - 1.0) as usize, (r + 1.0) as usize);
        let j = (cy * w + cx) * 4;
        assert!(img[j] > 200 && img[j + 1] < 90 && img[j + 2] < 90 && img[j + 3] == 255, "{:?}", &img[j..j + 4]);
    }

    #[test]
    fn dot_on_transparent_background_is_opaque_inside() {
        let mut img = vec![0u8; 16 * 16 * 4];
        draw_dot(&mut img, 16, 16);
        assert!(img.chunks(4).any(|p| p[3] == 255));
        draw_dot(&mut [0u8; 4], 1, 1); // za mały obraz – bez paniki
    }

    #[test]
    fn builds_unity_args_and_tooltip() {
        let a = unity_args("pl.rustmail.app", 3);
        assert_eq!(a[6], "'application://pl.rustmail.app.desktop'");
        assert_eq!(a[7], "{'count': <int64 3>, 'count-visible': <true>}");
        assert!(unity_args("x", 0)[7].contains("<false>"));
        assert_eq!(tooltip(0), "RustMail");
        assert!(tooltip(5).ends_with(": 5"));
    }
}
