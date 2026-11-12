//! Pogoda: opcjonalna (domyślnie wyłączona). Miejscowość wybiera użytkownik; program nie ustala położenia
//! sam (bez geolokalizacji i bez adresu IP jako źródła). Zapytania idą do Open-Meteo (bez konta i klucza),
//! a nic z pogody nie jest zapisywane na dysku ani wysyłane gdziekolwiek indziej.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::auth::AppState;

const GEO_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub region: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: String,
    pub code: i64,
    pub max: f64,
    pub min: f64,
    pub rain: Option<f64>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub temp: f64,
    pub feels: f64,
    pub code: i64,
    pub wind: f64,
    pub humidity: f64,
    pub is_day: bool,
    pub days: Vec<Day>,
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

pub fn parse_places(v: &Value) -> Vec<Place> {
    let Some(list) = v.get("results").and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|r| {
            let lat = r.get("latitude")?.as_f64()?;
            let lon = r.get("longitude")?.as_f64()?;
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return None;
            }
            let text = |k: &str| r.get(k).and_then(Value::as_str).map(|s| clean(s, 80)).unwrap_or_default();
            let name = text("name");
            if name.is_empty() {
                return None;
            }
            Some(Place { name, region: text("admin1"), country: text("country"), lat, lon })
        })
        .take(8)
        .collect()
}

pub fn parse_weather(v: &Value) -> Result<Weather, String> {
    let bad = || "Serwis pogody zwrócił nieoczekiwane dane.".to_string();
    let cur = v.get("current").ok_or_else(bad)?;
    let num = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64);
    let temp = num(cur, "temperature_2m").ok_or_else(bad)?;
    let code = cur.get("weather_code").and_then(Value::as_i64).ok_or_else(bad)?;
    let mut days = Vec::new();
    if let Some(d) = v.get("daily") {
        let arr = |k: &str| d.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        let (time, codes, max, min, rain) = (arr("time"), arr("weather_code"), arr("temperature_2m_max"), arr("temperature_2m_min"), arr("precipitation_probability_max"));
        for i in 0..time.len().min(7) {
            let (Some(date), Some(c), Some(mx), Some(mn)) = (
                time[i].as_str(),
                codes.get(i).and_then(Value::as_i64),
                max.get(i).and_then(Value::as_f64),
                min.get(i).and_then(Value::as_f64),
            ) else { continue };
            days.push(Day { date: clean(date, 10), code: c, max: mx, min: mn, rain: rain.get(i).and_then(Value::as_f64) });
        }
    }
    Ok(Weather {
        temp,
        feels: num(cur, "apparent_temperature").unwrap_or(temp),
        code,
        wind: num(cur, "wind_speed_10m").unwrap_or(0.0),
        humidity: num(cur, "relative_humidity_2m").unwrap_or(0.0),
        is_day: cur.get("is_day").and_then(Value::as_i64).unwrap_or(1) != 0,
        days,
    })
}

/// Szuka miejscowości po nazwie (wybór zapisuje użytkownik po stronie interfejsu).
#[tauri::command]
pub async fn weather_search(state: State<'_, AppState>, query: String) -> Result<Vec<Place>, String> {
    let q = clean(query.trim(), 80);
    if q.chars().count() < 2 {
        return Err("Wpisz nazwę miejscowości (co najmniej 2 litery).".to_string());
    }
    let resp = state
        .http
        .get(GEO_URL)
        .query(&[("name", q.as_str()), ("count", "8"), ("language", "pl"), ("format", "json")])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "Nie udało się połączyć z serwisem pogody.".to_string())?;
    if !resp.status().is_success() {
        return Err("Serwis pogody nie odpowiada.".to_string());
    }
    let v: Value = resp.json().await.map_err(|_| "Serwis pogody zwrócił nieoczekiwane dane.".to_string())?;
    Ok(parse_places(&v))
}

/// Pogoda dla współrzędnych wybranej miejscowości (aktualna i prognoza na kilka dni).
#[tauri::command]
pub async fn weather_get(state: State<'_, AppState>, lat: f64, lon: f64) -> Result<Weather, String> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Err("Niepoprawne współrzędne.".to_string());
    }
    let resp = state
        .http
        .get(FORECAST_URL)
        .query(&[
            ("latitude", format!("{lat:.3}")),
            ("longitude", format!("{lon:.3}")),
            ("current", "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day".to_string()),
            ("daily", "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max".to_string()),
            ("timezone", "auto".to_string()),
            ("forecast_days", "5".to_string()),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "Nie udało się połączyć z serwisem pogody.".to_string())?;
    if !resp.status().is_success() {
        return Err("Serwis pogody nie odpowiada.".to_string());
    }
    let v: Value = resp.json().await.map_err(|_| "Serwis pogody zwrócił nieoczekiwane dane.".to_string())?;
    parse_weather(&v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_places_and_rejects_bad_coords() {
        let v = json!({"results":[
            {"name":"Kraków","admin1":"Małopolskie","country":"Polska","latitude":50.06,"longitude":19.94},
            {"name":"X","latitude":123.0,"longitude":1.0},
            {"name":"","latitude":1.0,"longitude":1.0}
        ]});
        let p = parse_places(&v);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "Kraków");
        assert!(parse_places(&json!({})).is_empty());
    }

    #[test]
    fn parses_weather() {
        let v = json!({
            "current":{"temperature_2m":8.5,"apparent_temperature":6.0,"weather_code":61,"wind_speed_10m":12.0,"relative_humidity_2m":80,"is_day":0},
            "daily":{"time":["2026-11-12","2026-11-13"],"weather_code":[61,3],"temperature_2m_max":[9.0,10.0],"temperature_2m_min":[3.0,4.0],"precipitation_probability_max":[70,10]}
        });
        let w = parse_weather(&v).unwrap();
        assert_eq!(w.code, 61);
        assert!(!w.is_day);
        assert_eq!(w.days.len(), 2);
        assert_eq!(w.days[1].rain, Some(10.0));
        assert!(parse_weather(&json!({"current":{}})).is_err());
    }
}
